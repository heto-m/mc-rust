//! Data pack JSON as the NBT vanilla's codecs would encode it.

use crate::{Compound, List, Value};

/// Deeper documents are refused; vanilla's own data stays far below.
pub const MAX_DEPTH: u32 = 64;

/// A JSON object (a registry entry file) as a compound; `None` for invalid JSON, another root type or too
/// deep a document.
pub fn compound_from_slice(bytes: &[u8]) -> Option<Compound> {
    let json: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    match from_json(&json)? {
        Value::Compound(c) => Some(c),
        _ => None,
    }
}

/// Booleans become bytes, integers ints (longs out of range), other numbers doubles; `null` has no NBT form.
pub fn from_json(json: &serde_json::Value) -> Option<Value> {
    convert(json, 0).ok().flatten()
}

struct TooDeep;

fn convert(json: &serde_json::Value, depth: u32) -> Result<Option<Value>, TooDeep> {
    use serde_json::Value as J;
    if depth > MAX_DEPTH {
        return Err(TooDeep);
    }
    Ok(Some(match json {
        J::Null => return Ok(None),
        J::Bool(b) => Value::Byte(i8::from(*b)),
        J::Number(n) => match n.as_i64() {
            Some(v) => i32::try_from(v).map_or(Value::Long(v), Value::Int),
            None => match n.as_f64() {
                Some(v) => Value::Double(v),
                None => return Ok(None),
            },
        },
        J::String(s) => Value::String(s.clone()),
        J::Array(items) => {
            let mut values = Vec::with_capacity(items.len());
            for item in items {
                values.extend(convert(item, depth + 1)?);
            }
            Value::List(List::from_values(values))
        }
        J::Object(map) => {
            let mut c = Compound::new();
            for (k, v) in map {
                if let Some(v) = convert(v, depth + 1)? {
                    c.insert(k.clone(), v);
                }
            }
            Value::Compound(c)
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn types_follow_vanilla_codecs() {
        let c = compound_from_slice(
            br##"{"b":true,"i":7,"l":5000000000,"d":0.8,"s":"x","n":null,
                "strs":["sender","content"],"ints":[1,2],"mixed":[1,"a"],"objs":[{"a":1}],"empty":[]}"##,
        )
        .unwrap();
        assert_eq!(c.get("b"), Some(&Value::Byte(1)));
        assert_eq!(c.get("i"), Some(&Value::Int(7)));
        assert_eq!(c.get("l"), Some(&Value::Long(5_000_000_000)));
        assert_eq!(c.get("d"), Some(&Value::Double(0.8)));
        assert_eq!(c.get_str("s"), Some("x"));
        assert!(c.get("n").is_none());
        assert_eq!(
            c.get_list("strs").unwrap().as_strings(),
            ["sender", "content"]
        );
        assert_eq!(c.get_list("ints").unwrap().as_ints(), [1, 2]);
        let mixed = c.get_list("mixed").unwrap().as_compounds();
        assert_eq!(mixed[0].get(""), Some(&Value::Int(1)));
        assert_eq!(mixed[1].get_str(""), Some("a"));
        assert_eq!(
            c.get_list("objs").unwrap().as_compounds()[0].get_i32("a"),
            Some(1)
        );
        assert_eq!(c.get_list("empty"), Some(&List::Empty));
        assert_eq!(c.get_f32("d"), Some(0.8));
        assert_eq!(c.get_f32("i"), Some(7.0));
    }

    #[test]
    fn non_objects_and_deep_documents_are_refused() {
        assert!(compound_from_slice(b"[1]").is_none());
        assert!(compound_from_slice(b"{").is_none());
        let deep = format!("{}1{}", "[".repeat(100), "]".repeat(100));
        let doc = format!("{{\"a\":{deep}}}");
        assert!(compound_from_slice(doc.as_bytes()).is_none());
    }
}
