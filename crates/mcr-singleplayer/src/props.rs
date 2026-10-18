use std::fmt::Write as _;

use crate::level::{Difficulty, GameMode, LevelInfo};

/// File in the server directory remembering mcr's own per-world choices.
pub const WORLD_META: &str = "mcr-world.properties";
pub const MAX_PLAYERS: u32 = 8;
const MAX_SEED_CHARS: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum WorldType {
    #[default]
    Normal,
    Flat,
    LargeBiomes,
    Amplified,
}

impl WorldType {
    pub fn preset(self) -> &'static str {
        match self {
            Self::Normal => "minecraft:normal",
            Self::Flat => "minecraft:flat",
            Self::LargeBiomes => "minecraft:large_biomes",
            Self::Amplified => "minecraft:amplified",
        }
    }
}

/// The Create New World screen's choices.
#[derive(Clone, Debug, PartialEq)]
pub struct NewWorld {
    pub name: String,
    pub game_mode: GameMode,
    pub hardcore: bool,
    pub difficulty: Difficulty,
    pub cheats: bool,
    pub world_type: WorldType,
    pub seed: String,
    pub structures: bool,
    pub bonus_chest: bool,
    /// Changed game rules as `(id, value)`, already validated.
    pub game_rules: Vec<(String, String)>,
    /// Known experimental feature packs.
    pub experiments: Vec<String>,
    /// Validated superflat `generator-settings` JSON.
    pub generator_settings: Option<String>,
}

impl Default for NewWorld {
    fn default() -> Self {
        Self {
            name: "New World".into(),
            game_mode: GameMode::Survival,
            hardcore: false,
            difficulty: Difficulty::Normal,
            cheats: false,
            world_type: WorldType::Normal,
            seed: String::new(),
            structures: true,
            bonus_chest: false,
            game_rules: Vec::new(),
            experiments: Vec::new(),
            generator_settings: None,
        }
    }
}

/// Settings that only matter while the server generates a new world.
#[derive(Clone, Debug, PartialEq)]
pub struct Creation {
    pub seed: String,
    pub world_type: WorldType,
    pub structures: bool,
    pub generator_settings: Option<String>,
    pub initial_enabled_packs: Option<String>,
}

/// Everything `server.properties` needs for one launch.
#[derive(Clone, Debug, PartialEq)]
pub struct LaunchProps {
    pub folder: String,
    pub port: u16,
    pub game_mode: GameMode,
    pub difficulty: Difficulty,
    pub hardcore: bool,
    pub view_distance: i32,
    pub simulation_distance: i32,
    pub motd: String,
    pub creation: Option<Creation>,
}

impl LaunchProps {
    pub fn for_new(folder: &str, world: &NewWorld, port: u16) -> Self {
        Self {
            folder: folder.to_owned(),
            port,
            game_mode: world.game_mode,
            difficulty: if world.hardcore {
                Difficulty::Hard
            } else {
                world.difficulty
            },
            hardcore: world.hardcore,
            view_distance: 10,
            simulation_distance: 10,
            motd: world.name.clone(),
            creation: Some(Creation {
                seed: world.seed.chars().take(MAX_SEED_CHARS).collect(),
                world_type: world.world_type,
                structures: world.structures,
                generator_settings: world
                    .generator_settings
                    .clone()
                    .filter(|_| world.world_type == WorldType::Flat),
                initial_enabled_packs: crate::packs::initial_enabled_packs(
                    &crate::packs::experiments(world.experiments.iter().map(String::as_str)),
                ),
            }),
        }
    }

    pub fn for_existing(folder: &str, level: &LevelInfo, port: u16) -> Self {
        Self {
            folder: folder.to_owned(),
            port,
            game_mode: level.game_mode,
            difficulty: level.difficulty,
            hardcore: level.hardcore,
            view_distance: 10,
            simulation_distance: 10,
            motd: level.name.clone(),
            creation: None,
        }
    }

    pub fn with_distances(mut self, view: i32, simulation: i32) -> Self {
        self.view_distance = view.clamp(2, 32);
        self.simulation_distance = simulation.clamp(5, 32);
        self
    }
}

/// Escapes like `java.util.Properties.store`, so no value can end its line or start another key.
fn escape(text: &str, is_key: bool) -> String {
    let mut out = String::with_capacity(text.len());
    for (i, c) in text.chars().enumerate() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\u{c}' => out.push_str("\\f"),
            '=' | ':' | '#' | '!' => {
                out.push('\\');
                out.push(c);
            }
            ' ' if i == 0 || is_key => out.push_str("\\ "),
            ' '..='~' => out.push(c),
            c => {
                let mut buf = [0u16; 2];
                for unit in c.encode_utf16(&mut buf) {
                    let _ = write!(out, "\\u{unit:04X}");
                }
            }
        }
    }
    out
}

pub fn escape_value(value: &str) -> String {
    escape(value, false)
}

pub fn escape_key(key: &str) -> String {
    escape(key, true)
}

fn is_ws(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\u{c}')
}

fn unescape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('f') => out.push('\u{c}'),
            Some('u') => {
                let unit = hex4(&mut chars);
                let low = || {
                    let mut ahead = chars.clone();
                    (ahead.next() == Some('\\') && ahead.next() == Some('u'))
                        .then(|| hex4(&mut ahead).map(|u| (u, ahead)))
                        .flatten()
                        .filter(|(u, _)| (0xDC00..0xE000).contains(u))
                };
                match unit {
                    Some(high @ 0xD800..0xDC00) => match low() {
                        Some((lo, ahead)) => {
                            chars = ahead;
                            out.extend(
                                char::decode_utf16([high, lo]).map(|c| c.unwrap_or('\u{fffd}')),
                            );
                        }
                        None => out.push('\u{fffd}'),
                    },
                    Some(u) => out.push(char::from_u32(u32::from(u)).unwrap_or('\u{fffd}')),
                    None => out.push('\u{fffd}'),
                }
            }
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

fn hex4(chars: &mut std::str::Chars<'_>) -> Option<u16> {
    let hex: String = chars.take(4).collect();
    if hex.len() != 4 {
        return None;
    }
    u16::from_str_radix(&hex, 16).ok()
}

fn odd_trailing_backslashes(line: &str) -> bool {
    line.chars().rev().take_while(|&c| c == '\\').count() % 2 == 1
}

/// Bound on the `server.properties` text mcr reads; the file may come from a downloaded world.
pub const MAX_PROPERTIES_BYTES: usize = 64 * 1024;
const MAX_LOGICAL_LINE: usize = 4096;
const MAX_ENTRIES: usize = 1000;

/// Reads at most [`MAX_PROPERTIES_BYTES`] of a properties file.
pub fn read_properties_file(path: &std::path::Path) -> Option<String> {
    use std::io::Read;
    let mut bytes = Vec::new();
    crate::safe::open_read(path)
        .ok()?
        .take(MAX_PROPERTIES_BYTES as u64)
        .read_to_end(&mut bytes)
        .ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

fn truncate_at(text: &str, max: usize) -> &str {
    let mut end = text.len().min(max);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// Reads key/value pairs with `java.util.Properties.load` semantics (continuations, escapes,
/// `=`/`:`/whitespace separators, `#`/`!` comments). Linear in the input, which is cut at
/// [`MAX_PROPERTIES_BYTES`]; logical lines stop growing at 4 KiB and at most 1000 entries are read.
pub fn parse_properties(text: &str) -> Vec<(String, String)> {
    let text = truncate_at(text, MAX_PROPERTIES_BYTES);
    let mut lines = text
        .split_inclusive(['\n', '\r'])
        .map(|l| l.strip_suffix(['\n', '\r']).unwrap_or(l));
    let mut out = Vec::new();
    while let Some(first) = lines.next() {
        if out.len() >= MAX_ENTRIES {
            break;
        }
        let first = first.trim_start_matches(is_ws);
        if first.is_empty() || first.starts_with(['#', '!']) {
            continue;
        }
        let mut logical = String::with_capacity(first.len().min(MAX_LOGICAL_LINE));
        let mut segment = first;
        loop {
            let continued = odd_trailing_backslashes(segment);
            let body = if continued {
                &segment[..segment.len() - 1]
            } else {
                segment
            };
            let room = MAX_LOGICAL_LINE.saturating_sub(logical.len());
            logical.push_str(truncate_at(body, room));
            if !continued {
                break;
            }
            match lines.next() {
                Some(next) => segment = next.trim_start_matches(is_ws),
                None => break,
            }
        }
        let mut key_end = logical.len();
        let mut escaped = false;
        for (i, c) in logical.char_indices() {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '=' || c == ':' || is_ws(c) {
                key_end = i;
                break;
            }
        }
        let key = unescape(&logical[..key_end]);
        let mut rest = logical[key_end..].trim_start_matches(is_ws);
        if let Some(r) = rest.strip_prefix(['=', ':']) {
            rest = r.trim_start_matches(is_ws);
        }
        out.push((key, unescape(rest)));
    }
    out
}

/// The managed keys (raw values) in a stable order; creation keys only on a world's first launch.
pub fn managed(props: &LaunchProps) -> Vec<(&'static str, String)> {
    let b = |v: bool| v.to_string();
    let mut keys = vec![
        ("server-ip", "127.0.0.1".to_owned()),
        ("server-port", props.port.to_string()),
        ("online-mode", b(false)),
        ("enforce-secure-profile", b(false)),
        ("prevent-proxy-connections", b(false)),
        ("max-players", MAX_PLAYERS.to_string()),
        ("spawn-protection", "0".into()),
        ("op-permission-level", "4".into()),
        ("white-list", b(false)),
        ("enforce-whitelist", b(false)),
        ("enable-query", b(false)),
        ("enable-rcon", b(false)),
        ("rcon.password", String::new()),
        ("management-server-enabled", b(false)),
        ("management-server-host", "localhost".into()),
        ("enable-jmx-monitoring", b(false)),
        ("enable-status", b(false)),
        ("hide-online-players", b(true)),
        ("broadcast-console-to-ops", b(false)),
        ("broadcast-rcon-to-ops", b(false)),
        ("require-resource-pack", b(false)),
        ("pause-when-empty-seconds", "-1".into()),
        ("network-compression-threshold", "-1".into()),
        ("allow-flight", b(true)),
        ("accepts-transfers", b(false)),
        ("enable-code-of-conduct", b(false)),
        ("log-ips", b(false)),
        ("force-gamemode", b(false)),
        ("level-name", props.folder.clone()),
        ("motd", props.motd.clone()),
        ("gamemode", props.game_mode.name().into()),
        ("difficulty", props.difficulty.name().into()),
        ("hardcore", b(props.hardcore)),
        ("view-distance", props.view_distance.to_string()),
        ("simulation-distance", props.simulation_distance.to_string()),
    ];
    if let Some(c) = &props.creation {
        keys.push(("level-seed", c.seed.clone()));
        keys.push(("level-type", c.world_type.preset().into()));
        keys.push(("generate-structures", b(c.structures)));
        if let Some(settings) = &c.generator_settings {
            keys.push(("generator-settings", settings.clone()));
        }
        if let Some(packs) = &c.initial_enabled_packs {
            keys.push(("initial-enabled-packs", packs.clone()));
        }
    }
    keys
}

/// Re-serializes the existing file's entries mcr does not manage, then writes every managed key
/// last; the existing file (possibly from a downloaded world) can never override a managed key.
pub fn render_properties(existing: Option<&str>, props: &LaunchProps) -> String {
    let keys = managed(props);
    let mut out = String::from("# Written by mcr for a local singleplayer server\n");
    for (k, v) in parse_properties(existing.unwrap_or_default()) {
        if keys.iter().any(|(m, _)| *m == k) {
            continue;
        }
        let _ = writeln!(out, "{}={}", escape_key(&k), escape_value(&v));
    }
    for (k, v) in keys {
        let _ = writeln!(out, "{}={}", escape_key(k), escape_value(&v));
    }
    out
}

/// `ops.json` making the player an operator at level 4, or no operators.
/// Only a valid offline name and hyphenated UUID can become an operator.
pub fn render_ops(op: Option<(&str, &str)>) -> String {
    let valid = |uuid: &str, name: &str| {
        (1..=16).contains(&name.len())
            && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            && uuid.len() == 36
            && uuid.bytes().enumerate().all(|(i, b)| {
                if matches!(i, 8 | 13 | 18 | 23) {
                    b == b'-'
                } else {
                    b.is_ascii_hexdigit()
                }
            })
    };
    match op.filter(|(uuid, name)| valid(uuid, name)) {
        Some((uuid, name)) => {
            let entry = serde_json::json!([{
                "uuid": uuid,
                "name": name,
                "level": 4,
                "bypassesPlayerLimit": true,
            }]);
            format!("{entry:#}\n")
        }
        None => "[]\n".into(),
    }
}

/// Whether cheats were chosen for this world, from mcr's sidecar file.
pub fn read_cheats(meta: &str) -> Option<bool> {
    meta.lines()
        .find_map(|l| match l.trim().strip_prefix("allow-commands=")? {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        })
}

pub fn render_meta(cheats: bool) -> String {
    format!("# mcr singleplayer settings\nallow-commands={cheats}\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(text: &str, key: &str) -> Vec<String> {
        parse_properties(text)
            .into_iter()
            .filter(|(k, _)| k == key)
            .map(|(_, v)| v)
            .collect()
    }

    #[test]
    fn new_world_maps_settings() {
        let world = NewWorld {
            name: "Test".into(),
            hardcore: true,
            difficulty: Difficulty::Easy,
            world_type: WorldType::Amplified,
            seed: "hello world".into(),
            structures: false,
            ..NewWorld::default()
        };
        let props = LaunchProps::for_new("Test", &world, 40123).with_distances(12, 40);
        let text = render_properties(None, &props);
        for (k, v) in [
            ("server-ip", "127.0.0.1"),
            ("server-port", "40123"),
            ("online-mode", "false"),
            ("enforce-secure-profile", "false"),
            ("spawn-protection", "0"),
            ("max-players", "8"),
            ("hardcore", "true"),
            ("difficulty", "hard"),
            ("gamemode", "survival"),
            ("level-type", "minecraft:amplified"),
            ("level-seed", "hello world"),
            ("generate-structures", "false"),
            ("view-distance", "12"),
            ("simulation-distance", "32"),
            ("broadcast-console-to-ops", "false"),
        ] {
            assert_eq!(value(&text, k), [v], "{k}");
        }
    }

    #[test]
    fn creation_extras_only_when_chosen() {
        let plain = render_properties(None, &LaunchProps::for_new("p", &NewWorld::default(), 1));
        assert!(value(&plain, "generator-settings").is_empty());
        assert!(value(&plain, "initial-enabled-packs").is_empty());
        let world = NewWorld {
            world_type: WorldType::Flat,
            generator_settings: Some(r#"{"layers":[],"biome":"minecraft:plains"}"#.into()),
            experiments: vec!["trade_rebalance".into(), "evil".into()],
            ..NewWorld::default()
        };
        let text = render_properties(None, &LaunchProps::for_new("f", &world, 1));
        assert_eq!(
            value(&text, "generator-settings"),
            [r#"{"layers":[],"biome":"minecraft:plains"}"#]
        );
        assert_eq!(
            value(&text, "initial-enabled-packs"),
            ["vanilla,trade_rebalance"]
        );
        let normal = NewWorld {
            world_type: WorldType::Normal,
            ..world
        };
        let text = render_properties(None, &LaunchProps::for_new("n", &normal, 1));
        assert!(value(&text, "generator-settings").is_empty(), "flat only");
    }

    #[test]
    fn existing_world_keeps_user_keys_and_drops_creation_keys() {
        let level = LevelInfo {
            name: "Old".into(),
            game_mode: GameMode::Creative,
            difficulty: Difficulty::Hard,
            ..LevelInfo::default()
        };
        let props = LaunchProps::for_existing("Old", &level, 1234);
        let existing =
            "#comment\nmotd=x\nserver-port=1\nonline-mode = true\nmax-tick-time=5\nlevel-seed=9\n";
        let text = render_properties(Some(existing), &props);
        assert_eq!(value(&text, "max-tick-time"), ["5"]);
        assert_eq!(value(&text, "server-port"), ["1234"]);
        assert_eq!(value(&text, "online-mode"), ["false"]);
        assert_eq!(value(&text, "motd"), ["Old"]);
        assert_eq!(value(&text, "gamemode"), ["creative"]);
        assert_eq!(value(&text, "difficulty"), ["hard"]);
        assert_eq!(
            value(&text, "level-seed"),
            ["9"],
            "user's seed line is kept as is"
        );
        assert!(!text.contains("online-mode = true"));
    }

    #[test]
    fn values_are_escaped() {
        assert_eq!(escape_value(" a\\b\nc"), "\\ a\\\\b\\nc");
        assert_eq!(escape_value("é😀"), "\\u00E9\\uD83D\\uDE00");
        let world = NewWorld {
            name: "x\ny".into(),
            ..NewWorld::default()
        };
        let text = render_properties(None, &LaunchProps::for_new("f", &world, 1));
        assert_eq!(value(&text, "motd"), ["x\ny"]);
        assert!(text.contains("motd=x\\ny\n"));
    }

    #[test]
    fn ops_and_meta() {
        let ops: serde_json::Value = serde_json::from_str(&render_ops(Some((
            "8667ba71-b85a-4004-af54-457a9734eed7",
            "Steve",
        ))))
        .unwrap();
        assert_eq!(ops[0]["name"], "Steve");
        assert_eq!(ops[0]["level"], 4);
        assert_eq!(render_ops(None), "[]\n");
        assert_eq!(read_cheats(&render_meta(true)), Some(true));
        assert_eq!(read_cheats(&render_meta(false)), Some(false));
        assert_eq!(read_cheats("x=1"), None);
        for (uuid, name) in [
            ("8667ba71-b85a-4004-af54-457a9734eed7", "a\"b"),
            ("8667ba71-b85a-4004-af54-457a9734eed7", ""),
            ("8667ba71-b85a-4004-af54-457a9734eed7", "seventeen_chars__"),
            ("not-a-uuid", "Steve"),
        ] {
            assert_eq!(render_ops(Some((uuid, name))), "[]\n", "{name}");
        }
    }

    fn lookup(text: &str, key: &str) -> Vec<String> {
        parse_properties(text)
            .into_iter()
            .filter(|(k, _)| k == key)
            .map(|(_, v)| v)
            .collect()
    }

    #[test]
    fn hostile_values_round_trip_and_never_add_keys() {
        let hostile = [
            "a\nonline-mode=true",
            "a\ronline-mode=true",
            "a\r\nserver-ip=0.0.0.0",
            "\\u000aenable-rcon=true",
            "x\\",
            "trailing\\\nenable-query=true",
            "k=v:w",
            "# not a comment",
            "! nor this",
            "  leading",
            "\u{c}form\tfeed",
            "日本 😀",
        ];
        for value in hostile {
            let world = NewWorld {
                name: value.into(),
                seed: value.into(),
                ..NewWorld::default()
            };
            let text = render_properties(None, &LaunchProps::for_new("f", &world, 7));
            assert_eq!(lookup(&text, "motd"), [value], "{value:?}");
            assert_eq!(
                lookup(&text, "level-seed"),
                [value.chars().take(64).collect::<String>()]
            );
            let parsed = parse_properties(&text);
            let keys: Vec<&str> = parsed.iter().map(|(k, _)| k.as_str()).collect();
            let expected: Vec<&str> = managed(&LaunchProps::for_new("f", &world, 7))
                .iter()
                .map(|(k, _)| *k)
                .collect();
            assert_eq!(keys, expected, "{value:?} added or lost a key");
        }
    }

    #[test]
    fn reader_follows_java_properties() {
        let text = "a=1\n  b : 2\nc 3\n#x=1\n!y=1\nd=con\\\n   tinued\ne=x\\\\\nf\\=g=h\n\\u0041=\\u00e9\nk\r\nl=\\\n";
        let got = parse_properties(text);
        let want = [
            ("a", "1"),
            ("b", "2"),
            ("c", "3"),
            ("d", "continued"),
            ("e", "x\\"),
            ("f=g", "h"),
            ("A", "é"),
            ("k", ""),
            ("l", ""),
        ];
        let got: Vec<(&str, &str)> = got.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        assert_eq!(got, want);
    }

    #[test]
    fn pathological_files_parse_in_bounded_time() {
        let started = std::time::Instant::now();
        let backslashes = "\\\\\\\n".repeat(200_000);
        assert!(parse_properties(&backslashes).len() <= 1);
        let long = format!("k={}\n", "\\\n".repeat(100_000));
        let parsed = parse_properties(&long);
        assert!(parsed.iter().all(|(_, v)| v.len() <= MAX_LOGICAL_LINE));
        let many = "a=b\n".repeat(100_000);
        assert_eq!(parse_properties(&many).len(), MAX_ENTRIES);
        let unicode = "x=".to_owned() + &"\\uD83D".repeat(100_000);
        assert!(parse_properties(&unicode)[0].1.chars().count() <= MAX_LOGICAL_LINE);
        let wide = "é".repeat(MAX_PROPERTIES_BYTES);
        assert!(parse_properties(&wide).len() <= 1);
        assert!(started.elapsed() < std::time::Duration::from_secs(2));

        let dir = crate::level::tests::temp_dir("props-cap");
        let path = dir.join("server.properties");
        std::fs::write(&path, "a=b\n".repeat(50_000)).unwrap();
        assert_eq!(
            read_properties_file(&path).unwrap().len(),
            MAX_PROPERTIES_BYTES
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn existing_file_cannot_override_forced_keys() {
        let level = LevelInfo::default();
        let props = LaunchProps::for_existing("w", &level, 5555);
        let existing = "server-ip=0.0.0.0\nonline\\-mode=true\nenable-rcon true\nmotd=a\\\n  enable-query=true\nrcon.password:hunter2\nmanagement-server-enabled=true\nmax-tick-time=5\nx=\\\n";
        let text = render_properties(Some(existing), &props);
        for (k, v) in [
            ("server-ip", "127.0.0.1"),
            ("online-mode", "false"),
            ("enable-rcon", "false"),
            ("enable-query", "false"),
            ("rcon.password", ""),
            ("management-server-enabled", "false"),
            ("server-port", "5555"),
            ("max-tick-time", "5"),
        ] {
            assert_eq!(lookup(&text, k), [v], "{k}");
        }
        assert!(lookup(&text, "x").iter().all(|v| v.is_empty()));
        let tail = parse_properties(&text);
        assert_eq!(tail.last().unwrap().0, managed(&props).last().unwrap().0);
    }
}
