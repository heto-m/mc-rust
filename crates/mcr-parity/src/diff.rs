use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use serde_json::Value;

use crate::jsonl::Case;

const MAX_SHOWN_CHARS: usize = 1500;

#[derive(Clone, Debug, PartialEq)]
pub struct Mismatch {
    pub id: u32,
    /// JSON path of the first differing field, `$` when a whole output is missing.
    pub path: String,
    /// The first difference under each differing top-level field, so one field cannot hide another.
    pub fields: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    pub cases: usize,
    pub matched: usize,
    pub mismatches: Vec<Mismatch>,
}

fn numbers_equal(a: &serde_json::Number, b: &serde_json::Number) -> bool {
    match (a.as_i64(), b.as_i64()) {
        (Some(x), Some(y)) => x == y,
        _ => a.as_f64() == b.as_f64(),
    }
}

fn index(path: &str, i: usize) -> String {
    format!("{path}[{i}]")
}

fn key(path: &str, k: &str) -> String {
    if k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !k.is_empty() {
        format!("{path}.{k}")
    } else {
        format!("{path}[{}]", Value::String(k.to_owned()))
    }
}

fn walk(a: &Value, b: &Value, path: &str) -> Option<String> {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => (!numbers_equal(x, y)).then(|| path.to_owned()),
        (Value::Array(x), Value::Array(y)) => {
            for (i, (u, v)) in x.iter().zip(y).enumerate() {
                if let Some(p) = walk(u, v, &index(path, i)) {
                    return Some(p);
                }
            }
            (x.len() != y.len()).then(|| index(path, x.len().min(y.len())))
        }
        (Value::Object(x), Value::Object(y)) => {
            let keys: BTreeSet<&String> = x.keys().chain(y.keys()).collect();
            keys.into_iter().find_map(|k| match (x.get(k), y.get(k)) {
                (Some(u), Some(v)) => walk(u, v, &key(path, k)),
                _ => Some(key(path, k)),
            })
        }
        _ => (a != b).then(|| path.to_owned()),
    }
}

/// The first differing field, depth first, object keys in sorted order; numbers compare by value.
pub fn first_difference(vanilla: &Value, mcr: &Value) -> Option<String> {
    walk(vanilla, mcr, "$")
}

/// The value at a path [`first_difference`] produced, if both sides have it.
pub fn at<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut v = value;
    let mut rest = path.strip_prefix('$')?;
    while !rest.is_empty() {
        if let Some(r) = rest.strip_prefix('.') {
            let end = r.find(['.', '[']).unwrap_or(r.len());
            v = v.get(&r[..end])?;
            rest = &r[end..];
        } else {
            let r = rest.strip_prefix('[')?;
            if let Some(r) = r.strip_prefix('"') {
                let end = r.find("\"]")?;
                v = v.get(&r[..end])?;
                rest = &r[end + 2..];
            } else {
                let end = r.find(']')?;
                v = v.get(r[..end].parse::<usize>().ok()?)?;
                rest = &r[end + 1..];
            }
        }
    }
    Some(v)
}

/// `$.lines[3].text` becomes `$.lines[].text`, to count mismatches by field.
pub fn field_class(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    let mut rest = path;
    while let Some(open) = rest.find('[') {
        out.push_str(&rest[..=open]);
        rest = &rest[open + 1..];
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if digits > 0 && rest[digits..].starts_with(']') {
            rest = &rest[digits..];
        }
    }
    out.push_str(rest);
    out
}

fn per_field(vanilla: &Value, mcr: &Value) -> Vec<String> {
    match (vanilla, mcr) {
        (Value::Object(v), Value::Object(m)) => {
            let keys: BTreeSet<&String> = v.keys().chain(m.keys()).collect();
            keys.into_iter()
                .filter_map(|k| match (v.get(k), m.get(k)) {
                    (Some(a), Some(b)) => walk(a, b, &key("$", k)),
                    _ => Some(key("$", k)),
                })
                .collect()
        }
        _ => first_difference(vanilla, mcr).into_iter().collect(),
    }
}

pub fn diff(cases: &[Case], vanilla: &BTreeMap<u32, Value>, mcr: &BTreeMap<u32, Value>) -> Report {
    let mut report = Report {
        cases: cases.len(),
        ..Report::default()
    };
    for case in cases {
        let fields = match (vanilla.get(&case.id), mcr.get(&case.id)) {
            (Some(v), Some(m)) => per_field(v, m),
            _ => vec!["$".to_owned()],
        };
        match fields.first() {
            None => report.matched += 1,
            Some(first) => report.mismatches.push(Mismatch {
                id: case.id,
                path: first.clone(),
                fields,
            }),
        }
    }
    report
}

fn shown(v: Option<&Value>) -> String {
    let Some(v) = v else {
        return "<missing>".to_owned();
    };
    let text = v.to_string();
    if text.chars().count() <= MAX_SHOWN_CHARS {
        return text;
    }
    let cut: String = text.chars().take(MAX_SHOWN_CHARS).collect();
    format!("{cut}… ({} chars)", text.chars().count())
}

impl Report {
    pub fn passed(&self) -> bool {
        self.mismatches.is_empty() && self.matched == self.cases
    }

    pub fn pass_rate(&self) -> f64 {
        if self.cases == 0 {
            1.0
        } else {
            self.matched as f64 / self.cases as f64
        }
    }

    /// Cases mismatching under each field class, most frequent first; a case counts once per field.
    pub fn classes(&self) -> Vec<(String, usize)> {
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for m in &self.mismatches {
            for f in &m.fields {
                *counts.entry(field_class(f)).or_default() += 1;
            }
        }
        let mut list: Vec<_> = counts.into_iter().collect();
        list.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        list
    }

    /// The first `max` mismatches with the case, both outputs and the first differing field.
    pub fn render(
        &self,
        name: &str,
        cases: &[Case],
        vanilla: &BTreeMap<u32, Value>,
        mcr: &BTreeMap<u32, Value>,
        max: usize,
    ) -> String {
        let mut s = String::new();
        let by_id: BTreeMap<u32, &Value> = cases.iter().map(|c| (c.id, &c.case)).collect();
        for m in self.mismatches.iter().take(max) {
            let (v, r) = (vanilla.get(&m.id), mcr.get(&m.id));
            let _ = writeln!(s, "[{name}] case {} differs at {}", m.id, m.path);
            let _ = writeln!(s, "  case:    {}", shown(by_id.get(&m.id).copied()));
            let _ = writeln!(s, "  vanilla: {}", shown(v));
            let _ = writeln!(s, "  mcr:     {}", shown(r));
            if m.path != "$" {
                let _ = writeln!(
                    s,
                    "  at {}: vanilla {} / mcr {}",
                    m.path,
                    shown(v.and_then(|v| at(v, &m.path))),
                    shown(r.and_then(|r| at(r, &m.path)))
                );
            }
        }
        if !self.mismatches.is_empty() {
            let _ = writeln!(s, "[{name}] mismatches by field:");
            for (class, n) in self.classes() {
                let _ = writeln!(s, "  {n:>6}  {class}");
            }
        }
        let _ = writeln!(
            s,
            "[{name}] {}/{} cases match ({:.2}%)",
            self.matched,
            self.cases,
            self.pass_rate() * 100.0
        );
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn first_difference_names_the_field() {
        let a = json!({"lines": [{"text": "ab", "width": 12}], "width": 12});
        assert_eq!(first_difference(&a, &a.clone()), None);
        let b = json!({"lines": [{"text": "ac", "width": 12}], "width": 12});
        assert_eq!(first_difference(&a, &b).as_deref(), Some("$.lines[0].text"));
        let c =
            json!({"lines": [{"text": "ab", "width": 12}, {"text": "", "width": 0}], "width": 12});
        assert_eq!(first_difference(&a, &c).as_deref(), Some("$.lines[1]"));
        assert_eq!(
            first_difference(&json!({"a": 1}), &json!({"b": 1})).as_deref(),
            Some("$.a")
        );
        assert_eq!(
            first_difference(&json!({"x y": 1}), &json!({"x y": 2})).as_deref(),
            Some("$[\"x y\"]")
        );
    }

    #[test]
    fn numbers_compare_by_value() {
        assert_eq!(first_difference(&json!(6), &json!(6.0)), None);
        assert!(first_difference(&json!(6), &json!(6.5)).is_some());
        assert!(first_difference(&json!(1), &json!("1")).is_some());
    }

    #[test]
    fn every_differing_field_is_counted() {
        let cases = vec![Case {
            id: 0,
            case: json!({}),
        }];
        let vanilla: BTreeMap<u32, Value> = [(0, json!({"a": [1], "b": 2, "c": 3}))].into();
        let mcr: BTreeMap<u32, Value> = [(0, json!({"a": [0], "b": 2, "c": 4}))].into();
        let r = diff(&cases, &vanilla, &mcr);
        assert_eq!(r.mismatches[0].path, "$.a[0]");
        assert_eq!(
            r.classes(),
            [("$.a[]".to_owned(), 1), ("$.c".to_owned(), 1)]
        );
    }

    #[test]
    fn at_follows_paths() {
        let v = json!({"lines": [{"text": "ab"}, {"text": "cd"}], "x y": [1, 2]});
        assert_eq!(at(&v, "$.lines[1].text"), Some(&json!("cd")));
        assert_eq!(at(&v, "$[\"x y\"][1]"), Some(&json!(2)));
        assert_eq!(at(&v, "$.lines[5]"), None);
        assert_eq!(at(&v, "$"), Some(&v));
    }

    #[test]
    fn report_counts_missing_outputs_and_classes() {
        let cases: Vec<Case> = (0..3)
            .map(|id| Case {
                id,
                case: json!({}),
            })
            .collect();
        let vanilla: BTreeMap<u32, Value> = [
            (0, json!({"w": 1})),
            (1, json!({"w": [1, 2]})),
            (2, json!(0)),
        ]
        .into();
        let mcr: BTreeMap<u32, Value> = [(0, json!({"w": 1})), (1, json!({"w": [1, 3]}))].into();
        let r = diff(&cases, &vanilla, &mcr);
        assert_eq!(r.matched, 1);
        assert_eq!(
            r.mismatches,
            [
                Mismatch {
                    id: 1,
                    path: "$.w[1]".into(),
                    fields: vec!["$.w[1]".into()],
                },
                Mismatch {
                    id: 2,
                    path: "$".into(),
                    fields: vec!["$".into()],
                }
            ]
        );
        assert!(!r.passed());
        assert_eq!(r.classes()[0].1, 1);
        assert_eq!(field_class("$.lines[12].text[3]"), "$.lines[].text[]");
        let text = r.render("t", &cases, &vanilla, &mcr, 5);
        assert!(text.contains("case 1 differs at $.w[1]"), "{text}");
        assert!(text.contains("vanilla 2 / mcr 3"), "{text}");
        assert!(text.contains("1/3 cases match"), "{text}");
    }
}
