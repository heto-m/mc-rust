//! `META-INF/neoforge.mods.toml`, read for the Mods screen and the dependency pre-check only. A bounded
//! line reader covers the subset mods use (tables, array tables, strings, bare values); anything else
//! only loses fields.

use super::scan::Dependency;

const MAX_LINES: usize = 4000;
const MAX_TABLES: usize = 512;
const MAX_VALUES: usize = 64;
const MAX_VALUE: usize = 2048;
const MAX_ALTERNATIVES: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    /// `""` for the root, the header otherwise (`mods`, `dependencies.jei`).
    pub name: String,
    pub values: Vec<(String, String)>,
}

impl Table {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.values
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

fn strip_comment(rest: &str) -> &str {
    rest.split('#').next().unwrap_or_default().trim()
}

/// A quoted value ending on this line, or the opening of a multi-line one.
enum Value {
    Done(String),
    Open { delim: &'static str, text: String },
}

fn parse_value(raw: &str) -> Option<Value> {
    let raw = raw.trim();
    for delim in ["\"\"\"", "'''"] {
        if let Some(rest) = raw.strip_prefix(delim) {
            return Some(match rest.find(delim) {
                Some(end) => Value::Done(rest[..end].to_owned()),
                None => Value::Open {
                    delim,
                    text: rest.to_owned(),
                },
            });
        }
    }
    if let Some(rest) = raw.strip_prefix('"') {
        let mut out = String::new();
        let mut chars = rest.chars();
        while let Some(c) = chars.next() {
            match c {
                '"' => return Some(Value::Done(out)),
                '\\' => match chars.next()? {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    other => out.push(other),
                },
                c => out.push(c),
            }
        }
        return None;
    }
    if let Some(rest) = raw.strip_prefix('\'') {
        return rest
            .find('\'')
            .map(|end| Value::Done(rest[..end].to_owned()));
    }
    let bare = strip_comment(raw);
    (!bare.is_empty() && !bare.starts_with('[') && !bare.starts_with('{'))
        .then(|| Value::Done(bare.to_owned()))
}

fn clamp(mut text: String) -> String {
    if text.len() > MAX_VALUE {
        let mut end = MAX_VALUE;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
    }
    text
}

pub fn parse_toml(text: &str) -> Vec<Table> {
    let mut tables = vec![Table {
        name: String::new(),
        values: Vec::new(),
    }];
    let mut open: Option<(String, &'static str, String)> = None;
    for line in text.lines().take(MAX_LINES) {
        if let Some((key, delim, mut acc)) = open.take() {
            match line.find(delim) {
                Some(end) => {
                    acc.push('\n');
                    acc.push_str(&line[..end]);
                    push(&mut tables, key, acc);
                }
                None => {
                    if acc.len() < MAX_VALUE {
                        acc.push('\n');
                        acc.push_str(line);
                    }
                    open = Some((key, delim, acc));
                }
            }
            continue;
        }
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            let header = strip_comment(line);
            let name = header
                .trim_start_matches('[')
                .trim_end_matches(']')
                .trim()
                .trim_matches('"');
            if tables.len() < MAX_TABLES {
                tables.push(Table {
                    name: name.to_owned(),
                    values: Vec::new(),
                });
            }
            continue;
        }
        let Some((key, raw)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim().trim_matches('"').to_owned();
        match parse_value(raw) {
            Some(Value::Done(v)) => push(&mut tables, key, v),
            Some(Value::Open { delim, text }) => open = Some((key, delim, text)),
            None => {}
        }
    }
    tables
}

fn push(tables: &mut [Table], key: String, value: String) {
    if let Some(t) = tables.last_mut()
        && t.values.len() < MAX_VALUES
    {
        t.values.push((key, clamp(value.trim().to_owned())));
    }
}

/// A Maven version range as the Fabric-style alternatives [`super::version::Range`] reads.
/// A bare version is Maven's soft requirement, which any version satisfies.
pub fn maven_range(text: &str) -> Vec<String> {
    let text: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    if text.is_empty() || text == "*" || !text.starts_with(['[', '(']) {
        return vec!["*".to_owned()];
    }
    let mut out = Vec::new();
    let mut rest = text.as_str();
    while !rest.is_empty() && out.len() < MAX_ALTERNATIVES {
        let Some(end) = rest.find([']', ')']) else {
            return vec!["<unreadable>".to_owned()];
        };
        let (set, tail) = rest.split_at(end + 1);
        rest = tail.trim_start_matches(',');
        let lower_inclusive = set.starts_with('[');
        let upper_inclusive = set.ends_with(']');
        let inner = &set[1..set.len() - 1];
        let alt = match inner.split_once(',') {
            None if lower_inclusive && upper_inclusive && !inner.is_empty() => format!("={inner}"),
            None => return vec!["<unreadable>".to_owned()],
            Some((lo, hi)) => {
                let mut parts = Vec::new();
                if !lo.is_empty() {
                    parts.push(format!("{}{lo}", if lower_inclusive { ">=" } else { ">" }));
                }
                if !hi.is_empty() {
                    parts.push(format!("{}{hi}", if upper_inclusive { "<=" } else { "<" }));
                }
                if parts.is_empty() {
                    "*".to_owned()
                } else {
                    parts.join(" ")
                }
            }
        };
        out.push(alt);
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub id: String,
    pub version: String,
    pub name: Option<String>,
    pub description: String,
    pub icon: Option<String>,
    pub depends: Vec<Dependency>,
    pub breaks: Vec<Dependency>,
}

pub fn parse_manifest(text: &str) -> Result<Manifest, String> {
    let tables = parse_toml(text);
    let main = tables
        .iter()
        .find(|t| t.name == "mods")
        .ok_or("no [[mods]] entry")?;
    let id = main.get("modId").ok_or("missing modId")?.to_owned();
    let mut depends = Vec::new();
    let mut breaks = Vec::new();
    let own = format!("dependencies.{id}");
    for t in tables.iter().filter(|t| t.name == own) {
        let Some(dep) = t.get("modId") else {
            continue;
        };
        if t.get("side")
            .is_some_and(|s| s.eq_ignore_ascii_case("server"))
        {
            continue;
        }
        let kind = match (t.get("type"), t.get("mandatory")) {
            (Some(kind), _) => kind.to_ascii_lowercase(),
            (None, Some("false")) => "optional".to_owned(),
            _ => "required".to_owned(),
        };
        let range = maven_range(t.get("versionRange").unwrap_or("*"));
        let entry = Dependency {
            id: dep.to_owned(),
            range,
        };
        match kind.as_str() {
            "required" => depends.push(entry),
            "incompatible" => breaks.push(entry),
            _ => {}
        }
    }
    Ok(Manifest {
        version: main.get("version").unwrap_or("0").to_owned(),
        name: main.get("displayName").map(str::to_owned),
        description: main.get("description").unwrap_or_default().to_owned(),
        icon: main
            .get("iconFile")
            .or_else(|| main.get("logoFile"))
            .map(str::to_owned),
        id,
        depends,
        breaks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const JEI: &str = r#"
modLoader="javafml" #mandatory
loaderVersion="[4,)"
license="The MIT License (MIT)"

[[mods]] #mandatory
modId="jei" #mandatory
version="31.15.0.82"
displayName="Just Enough Items"
iconFile="jei-icon.png"
description='''
JEI is an item and recipe viewing mod.
'''

[[dependencies.jei]]
    modId="mezz_config"
    type="required"
    versionRange="[0.6.3,1.0.0)"
    side="BOTH"

[[dependencies.jei]]
    modId="mezz_config_gui"
    type="optional"
    versionRange="[0.5.9,)"

[[dependencies.jei]]
    modId="minecraft"
    type="required"
    versionRange="[26.3]"

[[dependencies.jei]]
    modId="serverthing"
    type="required"
    side="SERVER"

[[dependencies.jei]]
    modId="badmod"
    type="incompatible"
    versionRange="(,2.0)"

[modproperties.jei]
    catalogueImageIcon="jei-icon.png"
"#;

    #[test]
    fn reads_mods_and_dependencies() {
        let m = parse_manifest(JEI).unwrap();
        assert_eq!(m.id, "jei");
        assert_eq!(m.version, "31.15.0.82");
        assert_eq!(m.name.as_deref(), Some("Just Enough Items"));
        assert_eq!(m.description, "JEI is an item and recipe viewing mod.");
        assert_eq!(m.icon.as_deref(), Some("jei-icon.png"));
        let deps: Vec<(&str, Vec<String>)> = m
            .depends
            .iter()
            .map(|d| (d.id.as_str(), d.range.clone()))
            .collect();
        assert_eq!(
            deps,
            [
                ("mezz_config", vec![">=0.6.3 <1.0.0".to_owned()]),
                ("minecraft", vec!["=26.3".to_owned()]),
            ]
        );
        assert_eq!(m.breaks.len(), 1);
        assert_eq!(m.breaks[0].range, ["<2.0"]);
    }

    #[test]
    fn dependency_type_defaults_to_required() {
        let text = "[[mods]]\nmodId = \"jade\"\nversion = \"${file.jarVersion}\"\n\
                    [[dependencies.jade]]\nmodId = \"neoforge\"\nversionRange = \"[26.3-beta, )\"\n\
                    [[dependencies.jade]]\nmodId = \"old\"\nmandatory = false\n";
        let m = parse_manifest(text).unwrap();
        assert_eq!(m.version, "${file.jarVersion}");
        assert_eq!(m.depends.len(), 1);
        assert_eq!(m.depends[0].range, [">=26.3-beta"]);
    }

    #[test]
    fn maven_ranges() {
        assert_eq!(maven_range("[1.0,2.0)"), [">=1.0 <2.0"]);
        assert_eq!(maven_range("(1.0,]"), [">1.0"]);
        assert_eq!(maven_range("[1.0]"), ["=1.0"]);
        assert_eq!(maven_range("1.0"), ["*"]);
        assert_eq!(maven_range(""), ["*"]);
        assert_eq!(maven_range("[1,2),[3,4]"), [">=1 <2", ">=3 <=4"]);
        assert_eq!(maven_range("[1,2"), ["<unreadable>"]);
        assert_eq!(maven_range("[,]"), ["*"]);
    }

    #[test]
    fn hostile_input_stays_bounded() {
        let mut text = String::from("[[mods]]\nmodId=\"x\"\ndescription='''\n");
        text.push_str(&"a".repeat(100_000));
        let m = parse_manifest(&text).unwrap();
        assert!(m.description.len() <= MAX_VALUE);
        let many = "[[t]]\n".repeat(10_000);
        assert!(parse_toml(&many).len() <= MAX_TABLES);
        assert!(parse_manifest("modId=\"x\"").is_err());
        assert!(parse_manifest("[[mods]]\nversion=\"1\"").is_err());
    }
}
