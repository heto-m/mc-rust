use std::collections::BTreeMap;

use mcr_assets::ResourceManager;
use rustc_hash::FxHashMap;
use serde::Deserialize;

use crate::error::Result;

pub const DEFAULT_LANGUAGE: &str = "en_us";
const MAX_TRANSLATIONS: usize = 1 << 17;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LanguageInfo {
    pub code: String,
    pub name: String,
    pub region: String,
    pub bidirectional: bool,
}

impl LanguageInfo {
    pub fn label(&self) -> String {
        format!("{} ({})", self.name, self.region)
    }
}

#[derive(Deserialize)]
struct RawInfo {
    name: String,
    region: String,
    #[serde(default)]
    bidirectional: bool,
}

#[derive(Deserialize)]
struct RawMeta {
    #[serde(default)]
    language: BTreeMap<String, RawInfo>,
}

#[derive(Deserialize, Default)]
struct Deprecated {
    #[serde(default)]
    removed: Vec<String>,
    #[serde(default)]
    renamed: BTreeMap<String, String>,
}

#[derive(Debug)]
pub struct Lang {
    map: FxHashMap<String, String>,
    code: String,
    rtl: bool,
    languages: Vec<LanguageInfo>,
}

impl Default for Lang {
    fn default() -> Self {
        Self {
            map: FxHashMap::default(),
            code: DEFAULT_LANGUAGE.to_owned(),
            rtl: false,
            languages: vec![default_info()],
        }
    }
}

fn default_info() -> LanguageInfo {
    LanguageInfo {
        code: DEFAULT_LANGUAGE.to_owned(),
        name: "English".to_owned(),
        region: "US".to_owned(),
        bidirectional: false,
    }
}

/// Languages declared by any pack's `pack.mcmeta`, the highest-priority declaration winning.
pub fn available_languages(resources: &ResourceManager) -> Vec<LanguageInfo> {
    let mut found: BTreeMap<String, LanguageInfo> = BTreeMap::new();
    let stack = resources.read_stack("pack.mcmeta").unwrap_or_default();
    for bytes in stack.iter().rev() {
        let Ok(meta) = serde_json::from_slice::<RawMeta>(bytes) else {
            continue;
        };
        for (code, raw) in meta.language {
            found.entry(code.clone()).or_insert(LanguageInfo {
                code,
                name: raw.name,
                region: raw.region,
                bidirectional: raw.bidirectional,
            });
        }
    }
    found
        .entry(DEFAULT_LANGUAGE.to_owned())
        .or_insert_with(default_info);
    found.into_values().collect()
}

fn namespaces(resources: &ResourceManager, code: &str) -> Vec<String> {
    let suffix = format!("/lang/{code}.json");
    let mut out: Vec<String> = resources
        .list_prefix("assets/")
        .filter_map(|path| {
            let rest = path.strip_prefix("assets/")?;
            rest.strip_suffix(suffix.as_str()).map(str::to_owned)
        })
        .filter(|ns| !ns.contains('/'))
        .collect();
    out.sort_by(|a, b| (a != "minecraft", a).cmp(&(b != "minecraft", b)));
    out
}

fn append(resources: &ResourceManager, code: &str, map: &mut FxHashMap<String, String>) -> bool {
    let mut any = false;
    for ns in namespaces(resources, code) {
        let path = format!("assets/{ns}/lang/{code}.json");
        for bytes in resources.read_stack(&path).unwrap_or_default() {
            match serde_json::from_slice::<FxHashMap<String, serde_json::Value>>(&bytes) {
                Ok(pairs) => {
                    any = true;
                    for (key, value) in pairs {
                        if let serde_json::Value::String(text) = value
                            && (map.len() < MAX_TRANSLATIONS || map.contains_key(&key))
                        {
                            map.insert(key, rewrite_unsupported_formats(text));
                        }
                    }
                }
                Err(e) => tracing::warn!("unreadable {path}: {e}"),
            }
        }
    }
    any
}

impl mcr_protocol::types::text::Translate for Lang {
    fn translate(&self, key: &str) -> Option<&str> {
        self.get(key)
    }
}

/// Vanilla's `%(\d+\$)?[\d.]*[df]` to `%$1s` rewrite of lang values.
fn rewrite_unsupported_formats(text: String) -> String {
    if !text.contains('%') {
        return text;
    }
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut copied = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'%' {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        let digits = bytes[j..].iter().take_while(|b| b.is_ascii_digit()).count();
        let positional = digits > 0 && bytes.get(j + digits) == Some(&b'$');
        let group = if positional { j..j + digits + 1 } else { j..j };
        if positional {
            j += digits + 1;
        }
        j += bytes[j..]
            .iter()
            .take_while(|b| b.is_ascii_digit() || **b == b'.')
            .count();
        if matches!(bytes.get(j), Some(b'd' | b'f')) {
            out.push_str(&text[copied..i + 1]);
            out.push_str(&text[group]);
            out.push('s');
            copied = j + 1;
            i = j + 1;
        } else {
            i += 1;
        }
    }
    out.push_str(&text[copied..]);
    out
}

impl Lang {
    /// `en_us` first, the selected language on top, then the renamed and removed key table.
    pub fn load(resources: &ResourceManager, code: &str) -> Result<Self> {
        let languages = available_languages(resources);
        let mut map = FxHashMap::default();
        if !append(resources, DEFAULT_LANGUAGE, &mut map) {
            tracing::warn!("missing {DEFAULT_LANGUAGE} translations; falling back to keys");
        }
        let mut selected = languages.iter().find(|l| l.code == code);
        if code != DEFAULT_LANGUAGE {
            match selected {
                Some(_) if append(resources, code, &mut map) => {}
                _ => {
                    tracing::warn!("no translations for {code}; using {DEFAULT_LANGUAGE}");
                    selected = None;
                }
            }
        }
        let (code, rtl) =
            selected.map_or((DEFAULT_LANGUAGE, false), |l| (&l.code, l.bidirectional));
        if let Ok(Some(bytes)) = resources.read("assets/minecraft/lang/deprecated.json") {
            let table: Deprecated = serde_json::from_slice(&bytes).unwrap_or_default();
            apply_deprecated(&table, &mut map);
        }
        Ok(Self {
            map,
            code: code.to_owned(),
            rtl,
            languages,
        })
    }

    pub fn from_pairs<'a>(pairs: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        Self {
            map: pairs
                .into_iter()
                .map(|(k, v)| (k.to_owned(), v.to_owned()))
                .collect(),
            ..Self::default()
        }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.map.get(key).map(String::as_str)
    }

    pub fn code(&self) -> &str {
        &self.code
    }

    pub fn is_right_to_left(&self) -> bool {
        self.rtl
    }

    pub fn languages(&self) -> &[LanguageInfo] {
        &self.languages
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

fn apply_deprecated(table: &Deprecated, map: &mut FxHashMap<String, String>) {
    for key in &table.removed {
        map.remove(key);
    }
    for (from, to) in &table.renamed {
        match map.remove(from) {
            Some(text) => {
                map.insert(to.clone(), text);
            }
            None => {
                map.remove(to);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn unsupported_formats_become_string_slots() {
        for (from, to) in [
            ("%d HK", "%s HK"),
            ("%1$d of %2$.2f", "%1$s of %2$s"),
            ("%%d", "%%s"),
            ("100%", "100%"),
            ("%5s %x", "%5s %x"),
        ] {
            assert_eq!(rewrite_unsupported_formats(from.to_owned()), to);
        }
    }

    use mcr_assets::AssetSource;

    use super::*;

    #[derive(Debug)]
    struct Files(Vec<(&'static str, String)>);

    impl AssetSource for Files {
        fn read(&self, path: &str) -> mcr_assets::Result<Option<Vec<u8>>> {
            Ok(self
                .0
                .iter()
                .find(|(p, _)| *p == path)
                .map(|(_, body)| body.clone().into_bytes()))
        }

        fn list(&self) -> mcr_assets::Result<Vec<String>> {
            Ok(self.0.iter().map(|(p, _)| (*p).to_owned()).collect())
        }
    }

    fn manager(packs: Vec<Vec<(&'static str, &str)>>) -> ResourceManager {
        let mut rm = ResourceManager::new();
        for (i, files) in packs.into_iter().enumerate() {
            let files = files.into_iter().map(|(p, b)| (p, b.to_owned())).collect();
            rm.push_pack(format!("p{i}"), Box::new(Files(files)))
                .unwrap();
        }
        rm
    }

    const META: &str = r#"{"language":{
        "ja_jp":{"name":"Japanese","region":"Japan"},
        "ar_sa":{"name":"Arabic","region":"Saudi","bidirectional":true}}}"#;

    #[test]
    fn selected_language_falls_back_to_english_per_key() {
        let rm = manager(vec![vec![
            ("pack.mcmeta", META),
            ("assets/minecraft/lang/en_us.json", r#"{"a":"A","b":"B"}"#),
            ("assets/minecraft/lang/ja_jp.json", r#"{"a":"ア"}"#),
        ]]);
        let lang = Lang::load(&rm, "ja_jp").unwrap();
        assert_eq!(lang.get("a"), Some("ア"));
        assert_eq!(lang.get("b"), Some("B"));
        assert_eq!(lang.code(), "ja_jp");
    }

    #[test]
    fn later_pack_overrides_keys_not_files() {
        let rm = manager(vec![
            vec![("assets/minecraft/lang/en_us.json", r#"{"a":"A","b":"B"}"#)],
            vec![("assets/minecraft/lang/en_us.json", r#"{"a":"pack"}"#)],
        ]);
        let lang = Lang::load(&rm, "en_us").unwrap();
        assert_eq!(lang.get("a"), Some("pack"));
        assert_eq!(lang.get("b"), Some("B"));
    }

    #[test]
    fn unknown_or_empty_language_uses_english() {
        let rm = manager(vec![vec![
            ("pack.mcmeta", META),
            ("assets/minecraft/lang/en_us.json", r#"{"a":"A"}"#),
        ]]);
        for code in ["xx_xx", "ja_jp"] {
            let lang = Lang::load(&rm, code).unwrap();
            assert_eq!(lang.code(), "en_us");
            assert_eq!(lang.get("a"), Some("A"));
        }
    }

    #[test]
    fn language_list_is_sorted_and_carries_direction() {
        let rm = manager(vec![vec![("pack.mcmeta", META)]]);
        let list = available_languages(&rm);
        let codes: Vec<_> = list.iter().map(|l| l.code.as_str()).collect();
        assert_eq!(codes, ["ar_sa", "en_us", "ja_jp"]);
        assert!(list[0].bidirectional);
        assert_eq!(list[2].label(), "Japanese (Japan)");
    }

    #[test]
    fn right_to_left_flag_follows_the_selection() {
        let rm = manager(vec![vec![
            ("pack.mcmeta", META),
            ("assets/minecraft/lang/en_us.json", "{}"),
            ("assets/minecraft/lang/ar_sa.json", r#"{"a":"x"}"#),
        ]]);
        assert!(Lang::load(&rm, "ar_sa").unwrap().is_right_to_left());
        assert!(!Lang::load(&rm, "en_us").unwrap().is_right_to_left());
    }

    #[test]
    fn deprecated_keys_are_renamed_and_removed() {
        let rm = manager(vec![vec![
            (
                "assets/minecraft/lang/en_us.json",
                r#"{"old":"text","gone":"x","keep":"k","missing_to":"stale"}"#,
            ),
            (
                "assets/minecraft/lang/deprecated.json",
                r#"{"removed":["gone"],"renamed":{"old":"new","absent":"missing_to"}}"#,
            ),
        ]]);
        let lang = Lang::load(&rm, "en_us").unwrap();
        assert_eq!(lang.get("new"), Some("text"));
        assert_eq!(lang.get("old"), None);
        assert_eq!(lang.get("gone"), None);
        assert_eq!(lang.get("missing_to"), None);
        assert_eq!(lang.get("keep"), Some("k"));
    }

    #[test]
    fn extra_namespaces_merge_in() {
        let rm = manager(vec![vec![
            ("assets/minecraft/lang/en_us.json", r#"{"a":"A"}"#),
            ("assets/mod/lang/en_us.json", r#"{"m":"M"}"#),
        ]]);
        let lang = Lang::load(&rm, "en_us").unwrap();
        assert_eq!(lang.get("m"), Some("M"));
    }
}
