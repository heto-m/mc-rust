//! Text components: style merge and inheritance, legacy `§` codes, translation with arguments and
//! fallbacks, the plain string and the styled draw runs. Vanilla's `ComponentSerialization.CODEC`,
//! `Component.getString` and `StringDecomposer.iterateFormatted` against `mcr-protocol`'s component
//! parser and `to_plain_string`, and `mcr-ui`'s `flatten_chat`.

use mcr_nbt::{Compound, List, Value as Nbt};
use mcr_protocol::types::text::{ClickEvent, Component};
use mcr_ui::{Cell, InsertSpan, Lang, click_at, flatten_chat};
use serde_json::{Map, Value, json};

use crate::{Case, Env, Harness, ParityError, Result, Rng};

const CASES: usize = 4000;
const MAX_DEPTH: u32 = 4;
const MAX_CHILDREN: i64 = 3;
const MAX_ARGS: i64 = 4;

/// Keys from the jar's `en_us.json` (names only; the templates come from the jar on both sides).
const KEYS: &[&str] = &[
    "chat.type.text",
    "chat.type.announcement",
    "chat.type.emote",
    "chat.type.admin",
    "chat.square_brackets",
    "death.attack.player",
    "death.attack.arrow.item",
    "multiplayer.player.joined",
    "multiplayer.player.left",
    "commands.give.success.single",
    "commands.kill.success.single",
    "translation.test.args",
    "translation.test.complex",
    "translation.test.escape",
    "translation.test.invalid",
    "translation.test.invalid2",
    "translation.test.none",
    "translation.test.world",
    "gui.done",
    "options.on",
    "block.minecraft.stone",
    "item.minecraft.diamond",
];
const TEMPLATE_TOKENS: &[&str] = &[
    "%s", "%s", "%s", "%1$s", "%2$s", "%3$s", "%%", "%d", "%", "%5$s", "%0$s", "%x", "%.2f", "§c",
    "§l", "§r",
];
const NAMED: &[&str] = &[
    "black",
    "dark_blue",
    "dark_green",
    "dark_aqua",
    "dark_red",
    "dark_purple",
    "gold",
    "gray",
    "dark_gray",
    "blue",
    "green",
    "aqua",
    "red",
    "light_purple",
    "yellow",
    "white",
];
const FONTS: &[&str] = &[
    "minecraft:default",
    "minecraft:uniform",
    "minecraft:alt",
    "minecraft:illageralt",
];
const FLAGS: &[&str] = &[
    "bold",
    "italic",
    "underlined",
    "strikethrough",
    "obfuscated",
];
const CODES: &[char] = &[
    '0', '4', '9', 'a', 'c', 'e', 'f', 'k', 'l', 'm', 'n', 'o', 'r', 'L', 'R', 'z', ' ',
];
const SELECTORS: &[&str] = &["@p", "@a", "@s", "@e", "Steve"];

#[derive(Debug)]
pub struct TextHarness;

fn word(rng: &mut Rng, out: &mut String) {
    const POOLS: &[(u32, u32)] = &[
        (0x61, 0x7A),
        (0x61, 0x7A),
        (0x61, 0x7A),
        (0x41, 0x5A),
        (0x30, 0x39),
        (0x21, 0x2F),
        (0xC0, 0xFF),
        (0x4E00, 0x4E80),
    ];
    let (lo, hi) = *rng.pick(POOLS);
    for _ in 0..rng.range(1, 7) {
        out.push(char::from_u32(rng.range(i64::from(lo), i64::from(hi)) as u32).unwrap_or('?'));
        if rng.chance(6) {
            out.push('§');
            out.push(*rng.pick(CODES));
        }
    }
}

fn text(rng: &mut Rng) -> String {
    let mut s = String::new();
    for i in 0..rng.range(0, 4) {
        if i > 0 {
            s.push(' ');
        }
        if rng.chance(10) {
            s.push('§');
            s.push(*rng.pick(CODES));
        }
        word(rng, &mut s);
    }
    if rng.chance(3) {
        s.push('§');
    }
    s
}

fn template(rng: &mut Rng) -> String {
    let mut s = String::new();
    for _ in 0..rng.range(0, 6) {
        if rng.chance(45) {
            s.push_str(rng.pick(TEMPLATE_TOKENS));
        } else {
            word(rng, &mut s);
        }
        if rng.chance(50) {
            s.push(' ');
        }
    }
    s
}

fn color(rng: &mut Rng) -> Value {
    if rng.chance(60) {
        json!(rng.pick(NAMED))
    } else {
        json!(format!("#{:06X}", rng.range(0, 0xFF_FFFF)))
    }
}

fn style(rng: &mut Rng, c: &mut Map<String, Value>) {
    if rng.chance(30) {
        c.insert("color".into(), color(rng));
    }
    for flag in FLAGS {
        if rng.chance(12) {
            c.insert((*flag).into(), json!(rng.chance(60)));
        }
    }
    if rng.chance(6) {
        c.insert("font".into(), json!(rng.pick(FONTS)));
    }
    if rng.chance(5) {
        c.insert(
            "shadow_color".into(),
            json!(rng.range(i64::from(i32::MIN), i64::from(i32::MAX))),
        );
    }
    if rng.chance(6) {
        let value = format!("/v{}", rng.range(0, 99));
        let event = match rng.below(3) {
            0 => json!({"action": "run_command", "command": value}),
            1 => json!({"action": "suggest_command", "command": value}),
            _ => json!({"action": "copy_to_clipboard", "value": value}),
        };
        c.insert("click_event".into(), event);
    }
    if rng.chance(5) {
        c.insert(
            "insertion".into(),
            json!(format!("ins{}", rng.range(0, 99))),
        );
    }
}

/// A short decimal, so the case file's digits are the double both sides parse.
fn decimal(mantissa: i64, exponent: i32) -> Value {
    json!(
        format!("{mantissa}e{exponent}")
            .parse::<f64>()
            .unwrap_or(0.0)
    )
}

fn number(rng: &mut Rng) -> Value {
    match rng.below(9) {
        0 => json!(rng.range(-5, 5)),
        1 => json!(rng.range(-100_000, 100_000)),
        2 => json!(rng.range(i64::from(i32::MIN), i64::from(i32::MAX))),
        3 => json!(rng.range(-50, 50) as f64),
        4 => json!(rng.range(-400, 400) as f64 / 8.0),
        5 => decimal(rng.range(1, 999), 7),
        6 => decimal(rng.range(1, 999), -5),
        7 => decimal(rng.range(-1000, 1000), -1),
        _ => json!(true),
    }
}

fn arg(rng: &mut Rng, depth: u32) -> Value {
    match rng.below(100) {
        0..40 => json!(text(rng)),
        40..60 => number(rng),
        _ => component(rng, depth + 1, false),
    }
}

/// One component in the typed JSON both sides turn into NBT (integers `Int`, other numbers `Double`,
/// booleans `Byte`).
fn component(rng: &mut Rng, depth: u32, bare_ok: bool) -> Value {
    if bare_ok && rng.chance(8) {
        return json!(text(rng));
    }
    let mut c = Map::new();
    let deep = depth >= MAX_DEPTH;
    match rng.below(100) {
        0..55 => {
            c.insert("text".into(), json!(text(rng)));
        }
        55..90 if !deep => {
            let key = if rng.chance(55) {
                (*rng.pick(KEYS)).to_owned()
            } else {
                format!("mcr.missing.{}", rng.range(0, 9))
            };
            c.insert("translate".into(), json!(key));
            if rng.chance(55) {
                c.insert("fallback".into(), json!(template(rng)));
            }
            if rng.chance(80) {
                let args: Vec<Value> = (0..rng.range(0, MAX_ARGS))
                    .map(|_| arg(rng, depth))
                    .collect();
                if !args.is_empty() {
                    c.insert("with".into(), Value::Array(args));
                }
            }
        }
        55..90 => {
            c.insert("text".into(), json!(text(rng)));
        }
        90..94 => {
            c.insert(
                "score".into(),
                json!({"name": rng.pick(SELECTORS), "objective": "obj"}),
            );
        }
        94..97 => {
            c.insert("selector".into(), json!(rng.pick(SELECTORS)));
        }
        _ => {
            c.insert("nbt".into(), json!("Items[0]"));
            c.insert("entity".into(), json!("@s"));
        }
    }
    style(rng, &mut c);
    if !deep && rng.chance(35) {
        let extra: Vec<Value> = (0..rng.range(1, MAX_CHILDREN))
            .map(|_| component(rng, depth + 1, true))
            .collect();
        c.insert("extra".into(), Value::Array(extra));
    }
    Value::Object(c)
}

pub(crate) fn to_nbt(v: &Value) -> Result<Nbt> {
    Ok(match v {
        Value::Bool(b) => Nbt::Byte(i8::from(*b)),
        Value::Number(n) => match n.as_i64() {
            Some(i) => i32::try_from(i).map_or(Nbt::Long(i), Nbt::Int),
            None => Nbt::Double(n.as_f64().unwrap_or(0.0)),
        },
        Value::String(s) => Nbt::String(s.clone()),
        Value::Array(items) => Nbt::List(List::from_values(
            items.iter().map(to_nbt).collect::<Result<_>>()?,
        )),
        Value::Object(map) => {
            let mut c = Compound::new();
            for (k, v) in map {
                c.insert(k.as_str(), to_nbt(v)?);
            }
            Nbt::Compound(c)
        }
        Value::Null => return Err(ParityError::Invalid("null in a text case".into())),
    })
}

fn click_text(event: &ClickEvent) -> String {
    match event {
        ClickEvent::RunCommand(c) => format!("run_command:{c}"),
        ClickEvent::SuggestCommand(c) => format!("suggest_command:{c}"),
        ClickEvent::CopyToClipboard(v) => format!("copy_to_clipboard:{v}"),
        other => format!("{other:?}"),
    }
}

fn insertion_at(spans: &[InsertSpan], index: usize) -> Option<&str> {
    spans
        .iter()
        .filter(|s| s.range.contains(&index))
        .min_by_key(|s| s.range.len())
        .map(|s| s.text.as_str())
}

pub(crate) fn run_style(cell: &Cell, click: Option<&ClickEvent>, insertion: Option<&str>) -> Value {
    let s = cell.style;
    let mut out = json!({
        "bold": s.bold(),
        "italic": s.italic(),
        "underlined": s.underlined(),
        "strikethrough": s.strikethrough(),
        "obfuscated": s.obfuscated(),
        "font": format!("minecraft:{}", s.font.path()),
    });
    // Gson leaves out null fields, so absent values are left out here too.
    let optional = [
        ("color", s.color.map(|c| format!("#{c:06X}"))),
        ("shadow", s.shadow.map(|c| format!("#{c:08X}"))),
        ("click", click.map(click_text)),
        ("insertion", insertion.map(str::to_owned)),
    ];
    for (key, value) in optional {
        if let Some(v) = value {
            out[key] = json!(v);
        }
    }
    out
}

/// Consecutive characters with one style become one run.
pub(crate) fn runs(styled: impl Iterator<Item = (char, Value)>) -> Value {
    let mut out: Vec<(String, Value)> = Vec::new();
    for (ch, style) in styled {
        match out.last_mut() {
            Some((text, last)) if *last == style => text.push(ch),
            _ => out.push((ch.to_string(), style)),
        }
    }
    Value::Array(
        out.into_iter()
            .map(|(text, mut style)| {
                style["text"] = json!(text);
                style
            })
            .collect(),
    )
}

fn probe_case(lang: &Lang, case: &Case) -> Result<Value> {
    let nbt = to_nbt(&case.case["component"])?;
    let component = match Component::from_nbt(&nbt) {
        Ok(c) => c,
        Err(e) => return Ok(json!({"error": e.to_string()})),
    };
    let runs = crate::guarded(|| {
        let chat = flatten_chat(lang, &component, true);
        runs(chat.cells.iter().enumerate().map(|(i, cell)| {
            (
                cell.ch,
                run_style(
                    cell,
                    click_at(&chat.spans, i),
                    insertion_at(&chat.inserts, i),
                ),
            )
        }))
    });
    let plain = crate::guarded(|| json!(component.to_plain_string_with(lang)));
    Ok(json!({"plain": plain, "runs": runs}))
}

impl Harness for TextHarness {
    fn name(&self) -> &'static str {
        "text"
    }

    fn scope(&self) -> &'static [&'static str] {
        &[
            "crates/mcr-protocol/src/types/text.rs",
            "crates/mcr-ui/src/text.rs",
            "crates/mcr-ui/src/style.rs",
            "crates/mcr-ui/src/lang.rs",
            "crates/mcr-nbt/src",
            "crates/mcr-parity/src/text.rs",
        ]
    }

    fn count(&self) -> usize {
        CASES
    }

    fn case(&self, _id: usize, rng: &mut Rng) -> Value {
        json!({"component": component(rng, 0, true)})
    }

    fn probe(&self, env: &Env, cases: &[Case]) -> Result<Vec<Value>> {
        let resources = mcr_ui::vanilla_resources(&env.client_jar, None)?;
        let lang = Lang::load(&resources, mcr_ui::DEFAULT_LANGUAGE)?;
        cases.iter().map(|c| probe_case(&lang, c)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cases_cover_translation_edges() {
        let cases = crate::generate(&TextHarness);
        let all: String = cases.iter().map(|c| c.case.to_string()).collect();
        for needle in [
            "\"translate\"",
            "\"fallback\"",
            "\"with\"",
            "%1$s",
            "%%",
            "\"extra\"",
            "§",
            "\"score\"",
            "\"click_event\"",
            "translation.test.complex",
        ] {
            assert!(all.contains(needle), "{needle}");
        }
    }

    #[test]
    fn typed_json_becomes_nbt() {
        let v = to_nbt(&json!({"a": [1, "x"], "b": 2.0, "c": true, "d": [1, 2]})).unwrap();
        let c = v.as_compound().unwrap();
        assert_eq!(c.get("b"), Some(&Nbt::Double(2.0)));
        assert_eq!(c.get("c"), Some(&Nbt::Byte(1)));
        assert_eq!(c.get("d"), Some(&Nbt::List(List::Int(vec![1, 2]))));
        assert!(matches!(c.get("a"), Some(Nbt::List(List::Compound(_)))));
    }

    #[test]
    fn runs_merge_equal_styles() {
        let a = json!({"bold": true});
        let b = json!({"bold": false});
        let r = runs([('x', a.clone()), ('y', a), ('z', b)].into_iter());
        assert_eq!(r[0]["text"], "xy");
        assert_eq!(r[1]["text"], "z");
    }
}
