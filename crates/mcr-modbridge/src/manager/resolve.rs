use std::collections::HashMap;

use super::scan::{Dependency, ModInfo};
use super::version::{Range, Version};

pub const GAME_VERSION: &str = "26.3";
pub const LOADER_VERSION: &str = "0.19.5";
pub const JAVA_VERSION: &str = "25";
pub const BRIDGE_ID: &str = "mcr_bridge";
pub const BRIDGE_VERSION: &str = "0.1.0";

/// What the loader itself provides: the game, Fabric Loader, Java and the bridge mod.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostEnv {
    pub minecraft: String,
    pub loader: String,
    pub java: String,
}

impl Default for HostEnv {
    fn default() -> Self {
        Self {
            minecraft: GAME_VERSION.into(),
            loader: LOADER_VERSION.into(),
            java: JAVA_VERSION.into(),
        }
    }
}

impl HostEnv {
    fn provided(&self) -> [(&'static str, &str); 4] {
        [
            ("minecraft", &self.minecraft),
            ("fabricloader", &self.loader),
            ("java", &self.java),
            (BRIDGE_ID, BRIDGE_VERSION),
        ]
    }
}

type Providers = HashMap<String, Vec<Version>>;

fn providers(mods: &[&ModInfo], excluded: &[Option<String>], host: &HostEnv) -> Providers {
    let mut map = Providers::new();
    for (id, version) in host.provided() {
        map.entry(id.to_owned())
            .or_default()
            .push(Version::parse(version));
    }
    for (m, _) in mods.iter().zip(excluded).filter(|(_, e)| e.is_none()) {
        let version = Version::parse(&m.version);
        for id in std::iter::once(&m.id).chain(&m.provides) {
            map.entry(id.clone()).or_default().push(version.clone());
        }
        for n in &m.nested {
            let nested = Version::parse(&n.version);
            for id in std::iter::once(&n.id).chain(&n.provides) {
                map.entry(id.clone()).or_default().push(nested.clone());
            }
        }
    }
    map
}

fn describe(dep: &Dependency) -> String {
    dep.range.join(" or ")
}

fn satisfied(dep: &Dependency, found: &[Version]) -> Option<bool> {
    let range = Range::parse(&dep.range)?;
    Some(found.iter().any(|v| range.matches(v)))
}

fn found_versions(found: &[Version]) -> String {
    found
        .iter()
        .map(Version::raw)
        .collect::<Vec<_>>()
        .join(", ")
}

fn check(
    m: &ModInfo,
    all: &Providers,
    mods: &[&ModInfo],
    excluded: &[Option<String>],
    unavailable: &HashMap<String, String>,
) -> Option<String> {
    for dep in &m.depends {
        match all.get(&dep.id) {
            None => {
                let cannot_load = mods
                    .iter()
                    .zip(excluded)
                    .any(|(o, e)| e.is_some() && (o.id == dep.id || o.provides.contains(&dep.id)));
                return Some(match unavailable.get(&dep.id) {
                    Some(why) => format!("requires `{}`, which is {why}", dep.id),
                    None if cannot_load => format!("requires `{}`, which cannot be loaded", dep.id),
                    None => format!("missing dependency `{}`", dep.id),
                });
            }
            Some(found) => match satisfied(dep, found) {
                Some(true) => {}
                Some(false) => {
                    return Some(format!(
                        "needs `{}` {}, found {}",
                        dep.id,
                        describe(dep),
                        found_versions(found)
                    ));
                }
                None => return Some(format!("unreadable version range for `{}`", dep.id)),
            },
        }
    }
    for dep in &m.breaks {
        if dep.id == m.id {
            continue;
        }
        if let Some(found) = all.get(&dep.id)
            && satisfied(dep, found) == Some(true)
        {
            return Some(format!(
                "conflicts with `{}` {}",
                dep.id,
                found_versions(found)
            ));
        }
    }
    None
}

/// Per mod: `None` to load it, or the reason it is left out of this start. Excluding a mod can
/// orphan its dependents, so this repeats until nothing changes. `unavailable` names ids that
/// exist in the folder but are not loadable (blocked, disabled, ...), for clearer reasons.
pub fn resolve(
    mods: &[&ModInfo],
    unavailable: &HashMap<String, String>,
    host: &HostEnv,
) -> Vec<Option<String>> {
    let mut excluded: Vec<Option<String>> = vec![None; mods.len()];
    let mut seen: HashMap<&str, &str> = HashMap::new();
    for (i, m) in mods.iter().enumerate() {
        if let Some(first) = seen.get(m.id.as_str()) {
            excluded[i] = Some(format!("same id as {first}"));
        } else {
            seen.insert(&m.id, &m.file_name);
        }
    }
    loop {
        let mut changed = false;
        for i in 0..mods.len() {
            if excluded[i].is_some() {
                continue;
            }
            let all = providers(mods, &excluded, host);
            if let Some(reason) = check(mods[i], &all, mods, &excluded, unavailable) {
                excluded[i] = Some(reason);
                changed = true;
            }
        }
        if !changed {
            return excluded;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::scan::{ModKind, Nested, Side};
    use super::*;
    use std::path::PathBuf;

    fn info(id: &str, version: &str) -> ModInfo {
        ModInfo {
            path: PathBuf::from(format!("{id}.jar")),
            file_name: format!("{id}.jar"),
            kind: ModKind::Fabric,
            id: id.into(),
            name: id.into(),
            version: version.into(),
            description: String::new(),
            icon: None,
            side: Side::Both,
            depends: Vec::new(),
            recommends: Vec::new(),
            breaks: Vec::new(),
            provides: Vec::new(),
            nested: Vec::new(),
            warnings: Vec::new(),
        }
    }

    fn dep(id: &str, range: &[&str]) -> Dependency {
        Dependency {
            id: id.into(),
            range: range.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    fn run(mods: &[ModInfo]) -> Vec<Option<String>> {
        let refs: Vec<&ModInfo> = mods.iter().collect();
        resolve(&refs, &HashMap::new(), &HostEnv::default())
    }

    #[test]
    fn host_dependencies_are_checked() {
        let mut a = info("a", "1.0.0");
        a.depends = vec![
            dep("minecraft", &["~26.3"]),
            dep("fabricloader", &[">=0.19.3"]),
            dep("java", &[">=25"]),
            dep(BRIDGE_ID, &["*"]),
        ];
        assert_eq!(run(&[a.clone()]), [None]);
        a.depends.push(dep("java", &[">=26"]));
        let out = run(&[a]);
        assert!(
            out[0].as_ref().unwrap().contains("needs `java` >=26"),
            "{out:?}"
        );
    }

    #[test]
    fn missing_dependency_is_named() {
        let mut a = info("a", "1.0");
        a.depends = vec![dep("foo", &["*"])];
        assert_eq!(run(&[a]), [Some("missing dependency `foo`".to_owned())]);
    }

    #[test]
    fn version_ranges_use_alternatives() {
        let lib = info("lib", "2.5.0");
        let mut a = info("a", "1.0");
        a.depends = vec![dep("lib", &["1.x", ">=2.4 <3"])];
        assert_eq!(run(&[lib.clone(), a.clone()]), [None, None]);
        a.depends = vec![dep("lib", &["1.x", "3.x"])];
        let out = run(&[lib, a]);
        assert_eq!(out[0], None);
        assert_eq!(
            out[1].as_deref(),
            Some("needs `lib` 1.x or 3.x, found 2.5.0")
        );
    }

    #[test]
    fn provides_and_nested_ids_satisfy() {
        let mut host = info("host", "1.0");
        host.provides = vec!["alias".into()];
        host.nested = vec![Nested {
            id: "inner".into(),
            version: "3.1.0".into(),
            provides: vec!["inner-old".into()],
        }];
        let mut a = info("a", "1.0");
        a.depends = vec![
            dep("alias", &["1.0"]),
            dep("inner", &[">=3"]),
            dep("inner-old", &["*"]),
        ];
        assert_eq!(run(&[host, a]), [None, None]);
    }

    #[test]
    fn breaks_excludes_the_declaring_mod() {
        let other = info("other", "1.4.0");
        let mut a = info("a", "1.0");
        a.breaks = vec![dep("other", &["<2"])];
        let out = run(&[a.clone(), other.clone()]);
        assert_eq!(out[0].as_deref(), Some("conflicts with `other` 1.4.0"));
        assert_eq!(out[1], None);
        let newer = info("other", "2.0.0");
        assert_eq!(run(&[a, newer]), [None, None]);
    }

    #[test]
    fn exclusion_cascades_to_dependents() {
        let mut lib = info("lib", "1.0");
        lib.depends = vec![dep("missing", &["*"])];
        let mut app = info("app", "1.0");
        app.depends = vec![dep("lib", &["*"])];
        let out = run(&[lib, app]);
        assert_eq!(out[0].as_deref(), Some("missing dependency `missing`"));
        assert_eq!(
            out[1].as_deref(),
            Some("requires `lib`, which cannot be loaded")
        );
    }

    #[test]
    fn unavailable_ids_get_their_reason() {
        let mut a = info("a", "1.0");
        a.depends = vec![dep("sodium", &["*"])];
        let refs = [&a];
        let mut unavailable = HashMap::new();
        unavailable.insert("sodium".to_owned(), "blocked".to_owned());
        let out = resolve(&refs, &unavailable, &HostEnv::default());
        assert_eq!(
            out[0].as_deref(),
            Some("requires `sodium`, which is blocked")
        );
    }

    #[test]
    fn duplicate_ids_keep_the_first() {
        let a = info("a", "1.0");
        let mut b = info("a", "2.0");
        b.file_name = "a-2.jar".into();
        let out = run(&[a, b]);
        assert_eq!(out[0], None);
        assert_eq!(out[1].as_deref(), Some("same id as a.jar"));
    }

    #[test]
    fn mutual_breaks_drop_both_and_terminate() {
        let mut a = info("a", "1.0");
        let mut b = info("b", "1.0");
        a.breaks = vec![dep("b", &["*"])];
        b.breaks = vec![dep("a", &["*"])];
        let out = run(&[a, b]);
        assert!(out[0].is_some());
        assert!(
            out[1].is_none(),
            "the second sees the first already excluded: {out:?}"
        );
    }

    #[test]
    fn bad_ranges_exclude_instead_of_guessing() {
        let lib = info("lib", "1.0");
        let mut a = info("a", "1.0");
        a.depends = vec![dep("lib", &["not-a-range"])];
        let out = run(&[lib, a]);
        assert_eq!(
            out[1].as_deref(),
            Some("unreadable version range for `lib`")
        );
    }
}
