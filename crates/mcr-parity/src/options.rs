//! `options.txt` and key mappings: vanilla `Options` loads a generated file (data fixes included)
//! and saves it again; ours parses it with `OptionsFile::parse` and writes `to_text`. Each saved
//! key compares as its own field, plus the order of vanilla's keys, the key-binds screen's
//! collisions and, on a few cases, the key mapping table itself.

use mcr_settings::{InputKey, KeyBindings, MAPPINGS, OptionsFile, Toggle};
use serde_json::{Map, Value, json};

use crate::{Case, Env, Harness, Result, Rng};

const CASES: usize = 3000;

const CURRENT_VERSION: &str = "5023";

/// Below this data version `OptionsKeyLwjgl3Fix` reads key bindings as LWJGL 2 key codes.
const LWJGL3_VERSION: u32 = 1344;

/// Data versions around the options fixes, from LWJGL 3 key names to the graphics preset split.
const OLD_VERSIONS: &[&str] = &[
    "100", "504", "815", "1000", "1342", "1445", "1935", "2555", "2557", "3200", "3213", "3318",
    "3942", "4650", "4660", "4891",
];

/// Vanilla keys `Options.processOptions` reads that `Options` has no field for.
const VANILLA_ONLY: &[(&str, &str)] = &[("debugGuiScale", "0"), ("glDebugVerbosity", "1")];

const ENUMS: &[(&str, &[&str])] = &[
    ("graphicsPreset", &["fast", "fancy", "fabulous", "custom"]),
    ("preferredGraphicsBackend", &["default", "opengl", "vulkan"]),
    ("inactivityFpsLimit", &["minimized", "afk"]),
    ("mainHand", &["left", "right"]),
    ("sharePresence", &["none", "limited", "all"]),
    ("musicToast", &["never", "pause", "pause_and_toast"]),
    ("musicFrequency", &["DEFAULT", "FREQUENT", "CONSTANT"]),
    ("renderClouds", &["true", "fast", "false"]),
];

const ORDINALS: &[&str] = &[
    "particles",
    "narrator",
    "chatVisibility",
    "attackIndicator",
    "prioritizeChunkUpdates",
    "textureFiltering",
];

const LEGACY_LINES: &[&str] = &[
    "fancyGraphics:true",
    "fancyGraphics:false",
    "graphicsMode:0",
    "graphicsMode:1",
    "graphicsMode:2",
    "ao:0",
    "ao:1",
    "ao:2",
    "showNowPlayingToast:true",
    "showNowPlayingToast:false",
    "menuBackgroundBlurriness:0.25",
    "menuBackgroundBlurriness:1.0",
    "key_key.swapHands:key.keyboard.f",
    "key_key.swapHands:key.keyboard.r",
    "lang:en_US",
    "lang:fr_FR",
    "key_key.jump:57",
    "key_key.attack:-100",
    "key_key.inventory:18",
    "useVbo:true",
    "resourcePacks:[\"programmer_art\"]",
    "backgroundForChatOnly:false",
    "onboardAccessibility:false",
    "preferredGraphicsBackend:\"vulkan\"",
];

const UNKNOWN_LINES: &[&str] = &[
    "sodium.quality:1",
    "ofFastRender:true",
    ":nokey",
    "nocolon",
    "key_key.modded:key.keyboard.k",
    "soundCategory_bogus:0.5",
    "modelPart_tail:true",
    " fov:1.0",
    "fov :1.0",
    "fullscreenResolution:1920x1080@60:24",
    "fullscreenResolution:bogus",
    "mcrDynamicLights:false",
];

#[derive(Debug)]
pub struct OptionsHarness;

struct Shape {
    key: String,
    default: String,
}

fn shapes() -> Vec<Shape> {
    let text = OptionsFile::default().to_text();
    let mut out: Vec<Shape> = text
        .lines()
        .filter_map(|l| l.split_once(':'))
        .filter(|(k, _)| !k.starts_with("mcr"))
        .map(|(k, v)| Shape {
            key: k.to_owned(),
            default: v.to_owned(),
        })
        .collect();
    out.extend(VANILLA_ONLY.iter().map(|(k, v)| Shape {
        key: (*k).to_owned(),
        default: (*v).to_owned(),
    }));
    out
}

fn key_names() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = (1..400)
        .map(|c| InputKey::Keyboard(c).name())
        .chain((0..8).map(|c| InputKey::Mouse(c).name()))
        .filter(|n| *n != "key.keyboard.unknown")
        .collect();
    names.dedup();
    names
}

fn key_value(rng: &mut Rng, names: &[&'static str], lwjgl2: bool) -> String {
    if lwjgl2 && rng.chance(97) {
        return if rng.chance(85) {
            rng.range(0, 220).to_string()
        } else {
            rng.range(-100, -93).to_string()
        };
    }
    match rng.weighted(&[66, 15, 5, 4, 4, 2, 2, 2]) {
        0 => (*rng.pick(names)).to_owned(),
        1 => MAPPINGS[rng.below(MAPPINGS.len())]
            .default
            .name()
            .to_owned(),
        2 => "key.keyboard.unknown".to_owned(),
        3 => format!("key.keyboard.{}", rng.range(-5, 1000)),
        4 => format!("key.mouse.{}", rng.range(-2, 20)),
        5 => rng.range(-110, 255).to_string(),
        6 => (*rng.pick(&[
            "key.keyboard.x1",
            "scancode.42",
            "key.keyboard",
            "bogus",
            "",
        ]))
        .to_owned(),
        _ => (*rng.pick(names)).to_uppercase(),
    }
}

fn int_value(rng: &mut Rng, default: i64) -> String {
    match rng.weighted(&[35, 30, 20, 4, 3, 2, 2, 4]) {
        0 => (default + rng.range(-3, 3)).to_string(),
        1 => rng.range(0, 40).to_string(),
        2 => rng.range(-5, 300).to_string(),
        3 => (*rng.pick(&["2147483647", "-2147483648", "2147483648", "99999999999"])).to_owned(),
        4 => (*rng.pick(&["1.5", "4.0", "1e2", "-0"])).to_owned(),
        5 => (*rng.pick(&["", " 5", "+5", "0x10"])).to_owned(),
        6 => (*rng.pick(&["true", "false", "\"3\""])).to_owned(),
        _ => default.to_string(),
    }
}

fn float_value(rng: &mut Rng) -> String {
    let v = rng.range(-500, 1500) as f64 / 1000.0;
    match rng.weighted(&[45, 15, 10, 10, 6, 6, 5, 3]) {
        0 => format!("{v:?}"),
        1 => format!("{:?}", rng.range(-2, 12) as f64 * 0.5),
        2 => rng.range(-1, 12).to_string(),
        3 => (*rng.pick(&["1.0E-4", "5e-1", "0.30000000000000004", "-0.0", "1e400"])).to_owned(),
        4 => (*rng.pick(&["NaN", "Infinity", "-Infinity"])).to_owned(),
        5 => (*rng.pick(&["", "true", "false", "\"0.5\""])).to_owned(),
        6 => format!("{v:.20}"),
        _ => (*rng.pick(&[" 0.5", "0,5", ".5", "5."])).to_owned(),
    }
}

fn bool_value(rng: &mut Rng) -> String {
    match rng.weighted(&[85, 15]) {
        0 => (if rng.chance(50) { "true" } else { "false" }).to_owned(),
        _ => (*rng.pick(&["TRUE", "1", "0", "", "yes", " true", "\"true\""])).to_owned(),
    }
}

fn string_value(rng: &mut Rng) -> String {
    let base = *rng.pick(&[
        "",
        "a b",
        "dé",
        "龍",
        "x:y",
        "\\",
        "\"",
        "C:\\dir",
        "OpenAL Soft",
    ]);
    match rng.weighted(&[10, 85, 5]) {
        0 => base.to_owned(),
        1 => format!("\"{}\"", base.replace('\\', "\\\\").replace('"', "\\\"")),
        _ => format!("\"{base}\""),
    }
}

fn list_value(rng: &mut Rng) -> String {
    let valid = rng.chance(80);
    let values: &[&str] = if valid {
        &[
            "[]",
            "[\"vanilla\"]",
            "[\"vanilla\",\"file/x.zip\"]",
            "[\"a\" , \"b\"]",
            "[1,2]",
        ]
    } else {
        &[
            "\"single\"",
            "not json",
            "[\"unterminated",
            "",
            "['single']",
        ]
    };
    (*rng.pick(values)).to_owned()
}

fn value(rng: &mut Rng, shape: &Shape, names: &[&'static str], lwjgl2: bool) -> String {
    let key = shape.key.as_str();
    if key.starts_with("key_") {
        return key_value(rng, names, lwjgl2);
    }
    if let Some((_, values)) = ENUMS.iter().find(|(k, _)| *k == key) {
        let v = *rng.pick(values);
        return match rng.weighted(&[70, 10, 8, 6, 6]) {
            0 => format!("\"{v}\""),
            1 => v.to_owned(),
            2 => format!("\"{}\"", v.to_uppercase()),
            3 => "\"bogus\"".to_owned(),
            _ => (*rng.pick(&["", "1", "true", "false"])).to_owned(),
        };
    }
    if ORDINALS.contains(&key) {
        return if rng.chance(75) {
            rng.range(-1, 4).to_string()
        } else {
            int_value(rng, 0)
        };
    }
    match key {
        "lang" => {
            return (*rng.pick(&["en_us", "EN_US", "fr_fr", "", "zz_zz", "en_us "])).to_owned();
        }
        "tutorialStep" => {
            return (*rng.pick(&["movement", "none", "punch_tree", "find_tree", "bogus", ""]))
                .to_owned();
        }
        "lastServer" => {
            return (*rng.pick(&["", "localhost", "127.0.0.1:25565", "a b:c"])).to_owned();
        }
        "soundDevice" => return string_value(rng),
        _ => {}
    }
    let d = shape.default.as_str();
    if d == "true" || d == "false" {
        bool_value(rng)
    } else if d.starts_with('[') {
        list_value(rng)
    } else if d.starts_with('"') {
        string_value(rng)
    } else if let Ok(i) = d.parse::<i64>() {
        int_value(rng, i)
    } else if d.parse::<f64>().is_ok() {
        float_value(rng)
    } else {
        string_value(rng)
    }
}

fn generate_text(rng: &mut Rng, shapes: &[Shape], names: &[&'static str]) -> String {
    let mut lines: Vec<String> = Vec::new();
    let version = match rng.weighted(&[5, 70, 25]) {
        0 => None,
        1 => Some(CURRENT_VERSION),
        _ => Some(*rng.pick(OLD_VERSIONS)),
    };
    let lwjgl2 = version.is_none_or(|v| v.parse::<u32>().is_ok_and(|v| v < LWJGL3_VERSION));
    let full = rng.chance(25);
    for shape in shapes {
        let modify = rng.chance(if full { 8 } else { 3 });
        if full && !modify {
            lines.push(format!("{}:{}", shape.key, shape.default));
        } else if modify || rng.chance(12) {
            lines.push(format!(
                "{}:{}",
                shape.key,
                value(rng, shape, names, lwjgl2)
            ));
        }
    }
    let legacy = rng.range(
        0,
        if version.is_some_and(|v| v != CURRENT_VERSION) {
            4
        } else {
            1
        },
    );
    for _ in 0..legacy {
        let at = rng.below(lines.len() + 1);
        lines.insert(at, (*rng.pick(LEGACY_LINES)).to_owned());
    }
    if rng.chance(30) {
        for _ in 0..rng.range(1, 3) {
            let at = rng.below(lines.len() + 1);
            lines.insert(at, (*rng.pick(UNKNOWN_LINES)).to_owned());
        }
    }
    if rng.chance(15) && !shapes.is_empty() {
        let shape = &shapes[rng.below(shapes.len())];
        let at = rng.below(lines.len() + 1);
        lines.insert(
            at,
            format!("{}:{}", shape.key, value(rng, shape, names, lwjgl2)),
        );
    }
    if rng.chance(5) {
        let at = rng.below(lines.len() + 1);
        lines.insert(at, String::new());
    }
    if rng.chance(50) && !lines.is_empty() {
        let i = rng.below(lines.len());
        lines.swap(0, i);
    }
    if let Some(v) = version {
        lines.insert(0, format!("version:{v}"));
    }
    let eol = match rng.weighted(&[85, 12, 3]) {
        0 => "\n",
        1 => "\r\n",
        _ => "\r",
    };
    let mut text = lines.join(eol);
    if rng.chance(80) {
        text.push_str(eol);
    }
    if rng.chance(2) {
        text.insert(0, '\u{feff}');
    }
    text
}

/// Saved `key:value` lines as fields; `version` is left out since we never write one.
fn saved_fields(text: &str, out: &mut Map<String, Value>) {
    let mut order = Vec::new();
    let known: Vec<String> = shapes()
        .into_iter()
        .map(|s| s.key)
        .filter(|k| !VANILLA_ONLY.iter().any(|(v, _)| v == k))
        .collect();
    for line in text.split('\n') {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        // mcr's own keys are ours alone; vanilla drops them like any unknown key.
        if key == "version" || key.starts_with("mcr") {
            continue;
        }
        if known.iter().any(|k| k == key) {
            order.push(Value::from(key));
        }
        out.insert(key.to_owned(), Value::from(value));
    }
    out.insert("#order".to_owned(), Value::Array(order));
}

fn collisions(bindings: &KeyBindings) -> Value {
    let mut out = Map::new();
    for (i, m) in MAPPINGS.iter().enumerate() {
        let mut names: Vec<&str> = bindings
            .collisions(i)
            .into_iter()
            .map(|o| MAPPINGS[o].name)
            .collect();
        if names.is_empty() {
            continue;
        }
        names.sort_unstable();
        out.insert(m.name.to_owned(), json!(names));
    }
    Value::Object(out)
}

fn table() -> Value {
    MAPPINGS
        .iter()
        .map(|m| {
            json!({
                "name": m.name,
                "default": m.default.name(),
                "category": m.category.label_key().trim_start_matches("key.category.minecraft."),
                "order": m.order(),
                "toggle": m.toggle != Toggle::None,
            })
        })
        .collect()
}

fn probe_case(case: &Case) -> Value {
    let file = match case.case["text"].as_str() {
        Some(text) => OptionsFile::parse(text),
        None => OptionsFile::default(),
    };
    let mut out = Map::new();
    saved_fields(&file.to_text(), &mut out);
    out.insert("#collisions".to_owned(), collisions(&file.bindings));
    if case.case["table"].as_bool() == Some(true) {
        out.insert("#table".to_owned(), table());
    }
    Value::Object(out)
}

impl Harness for OptionsHarness {
    fn name(&self) -> &'static str {
        "options"
    }

    fn scope(&self) -> &'static [&'static str] {
        &["crates/mcr-settings/src"]
    }

    fn count(&self) -> usize {
        CASES
    }

    fn case(&self, id: usize, rng: &mut Rng) -> Value {
        let shapes = shapes();
        let names = key_names();
        let text = if rng.chance(2) {
            Value::Null
        } else {
            Value::from(generate_text(rng, &shapes, &names))
        };
        json!({"text": text, "table": id.is_multiple_of(500)})
    }

    fn probe(&self, _env: &Env, cases: &[Case]) -> Result<Vec<Value>> {
        Ok(cases
            .iter()
            .map(|c| crate::guarded(|| probe_case(c)))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cases_cover_legacy_unknown_and_missing_files() {
        let cases = crate::generate(&OptionsHarness);
        let texts: Vec<&str> = cases
            .iter()
            .filter_map(|c| c.case["text"].as_str())
            .collect();
        assert!(cases.iter().any(|c| c.case["text"].is_null()));
        assert!(texts.iter().any(|t| t.contains("version:1342")));
        assert!(texts.iter().any(|t| t.starts_with("version:5023")));
        assert!(texts.iter().any(|t| t.contains("fancyGraphics:")));
        assert!(texts.iter().any(|t| t.contains("sodium.quality:1")));
        assert!(texts.iter().any(|t| t.contains("\r\n")));
        assert!(texts.iter().any(|t| t.contains("debugGuiScale:")));
        assert!(cases.iter().any(|c| c.case["table"] == true));
    }

    #[test]
    fn defaults_save_every_known_key_once() {
        let out = probe_case(&Case {
            id: 0,
            case: json!({"text": null, "table": false}),
        });
        let order = out["#order"].as_array().unwrap();
        assert_eq!(order.len(), shapes().len() - VANILLA_ONLY.len());
        assert!(out.get("debugGuiScale").is_none());
    }
}
