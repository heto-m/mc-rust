//! Glyph advances, string width and line wrapping: vanilla's `Font` / `StringSplitter` (and the
//! chat wrapping of `ComponentRenderUtils`) against `mcr-ui`'s fonts, `Ui::width` and `wrap`.

use std::collections::HashSet;

use mcr_buf::Identifier;
use mcr_protocol::types::text::{Component, Style as TextStyle};
use mcr_ui::{Cell, ChatHud, ChatOptions, Chunk, Flags, FontId, Style, Ui, push_legacy};
use serde_json::{Value, json};

use crate::{Case, Env, Harness, ParityError, Result, Rng, round};

const CASES: usize = 3000;
const MAX_PARTS: i64 = 3;
const MAX_TOKENS: i64 = 12;
/// Vanilla's chat is never narrower than 40 (`chatWidth` 0) nor wider than 320.
const CHAT_WIDTHS: std::ops::RangeInclusive<i64> = 40..=320;
const STEP: f64 = 1.0 / 1024.0;

/// `(first, last, weight)`; `§` is left out, codes are inserted on purpose.
const POOLS: &[(u32, u32, u32)] = &[
    (0x61, 0x7A, 40),
    (0x41, 0x5A, 10),
    (0x30, 0x39, 6),
    (0x21, 0x2F, 4),
    (0x3A, 0x40, 2),
    (0x5B, 0x60, 2),
    (0x7B, 0x7E, 2),
    (0xA1, 0xA6, 1),
    (0xA8, 0xFF, 5),
    (0x100, 0x17F, 3),
    (0x391, 0x3C9, 2),
    (0x410, 0x44F, 3),
    (0x5D0, 0x5EA, 1),
    (0x621, 0x64A, 1),
    (0x300, 0x36F, 1),
    (0x2000, 0x206F, 2),
    (0x2190, 0x25FF, 2),
    (0x3041, 0x3096, 2),
    (0x4E00, 0x9FFF, 3),
    (0xAC00, 0xD7A3, 2),
    (0xE000, 0xE0FF, 1),
    (0x1F300, 0x1F64F, 2),
    (0x10000, 0x1FFFF, 1),
];
const SPECIALS: &[char] = &[
    '\u{A0}', '\u{200B}', '\u{3000}', '\u{FEFF}', '\u{FFFD}', '\u{FFFF}', '\t', '\r',
];
const CODES: &[char] = &[
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'a', 'b', 'c', 'd', 'e', 'f', 'k', 'l', 'm',
    'n', 'o', 'r', 'A', 'F', 'K', 'L', 'M', 'N', 'O', 'R', 'z', 'x', ' ', '§',
];
const FONTS: &[(Option<&str>, u32)] = &[
    (None, 50),
    (Some("minecraft:default"), 15),
    (Some("minecraft:uniform"), 12),
    (Some("minecraft:alt"), 8),
    (Some("minecraft:illageralt"), 8),
    (Some("minecraft:nonexistent"), 3),
];

#[derive(Debug)]
pub struct FontHarness;

fn pool_char(rng: &mut Rng, pool: usize) -> char {
    let (lo, hi, _) = POOLS[pool];
    let cp = rng.range(i64::from(lo), i64::from(hi)) as u32;
    char::from_u32(cp).unwrap_or('?')
}

fn word(rng: &mut Rng, out: &mut String) {
    let weights: Vec<u32> = POOLS.iter().map(|p| p.2).collect();
    let home = rng.weighted(&weights);
    let len = if rng.chance(10) {
        rng.range(12, 40)
    } else {
        rng.range(1, 10)
    };
    for _ in 0..len {
        let c = match rng.below(100) {
            0..80 => pool_char(rng, home),
            80..97 => {
                let pool = rng.weighted(&weights);
                pool_char(rng, pool)
            }
            _ => *rng.pick(SPECIALS),
        };
        out.push(c);
        if rng.chance(3) {
            legacy(rng, out);
        }
    }
}

fn legacy(rng: &mut Rng, out: &mut String) {
    out.push('§');
    out.push(*rng.pick(CODES));
}

fn part_text(rng: &mut Rng) -> String {
    let mut s = String::new();
    if rng.chance(8) {
        s.push(' ');
    }
    for _ in 0..rng.range(0, MAX_TOKENS) {
        if rng.chance(12) {
            legacy(rng, &mut s);
        }
        word(rng, &mut s);
        match rng.below(100) {
            0..70 => s.push(' '),
            70..80 => s.push_str("  "),
            80..87 => s.push('\n'),
            87..90 => s.push_str(" \n"),
            _ => {}
        }
    }
    if rng.chance(3) {
        s.push('§');
    }
    s
}

fn wrap_width(rng: &mut Rng) -> i64 {
    match rng.weighted(&[2, 10, 30, 40, 18]) {
        0 => rng.range(-2, 0),
        1 => rng.range(1, 10),
        2 => rng.range(11, 60),
        3 => rng.range(61, 200),
        _ => rng.range(201, 400),
    }
}

fn generate(rng: &mut Rng) -> Value {
    let weights: Vec<u32> = FONTS.iter().map(|f| f.1).collect();
    let parts: Vec<Value> = (0..rng.range(1, MAX_PARTS))
        .map(|_| {
            let font = FONTS[rng.weighted(&weights)].0;
            json!({"text": part_text(rng), "bold": rng.chance(25), "font": font})
        })
        .collect();
    json!({"parts": parts, "wrap": wrap_width(rng)})
}

struct Part {
    text: String,
    bold: bool,
    font: Option<String>,
}

fn parse(case: &Case) -> Result<(Vec<Part>, i32)> {
    let bad = |what: &str| ParityError::Invalid(format!("font case {}: {what}", case.id));
    let parts = case.case["parts"]
        .as_array()
        .ok_or_else(|| bad("no parts"))?
        .iter()
        .map(|p| {
            Ok(Part {
                text: p["text"]
                    .as_str()
                    .ok_or_else(|| bad("part text"))?
                    .to_owned(),
                bold: p["bold"].as_bool().unwrap_or(false),
                font: p["font"].as_str().map(str::to_owned),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let wrap = case.case["wrap"]
        .as_i64()
        .and_then(|w| i32::try_from(w).ok())
        .ok_or_else(|| bad("wrap"))?;
    Ok((parts, wrap))
}

fn font_name(font: FontId) -> String {
    format!("minecraft:{}", font.path())
}

fn cp(ch: char) -> String {
    format!("U+{:04X}", ch as u32)
}

fn plain(cells: &[Cell]) -> String {
    cells.iter().map(|c| c.ch).collect()
}

fn component(parts: &[Part]) -> Component {
    parts.iter().fold(Component::text(""), |root, p| {
        root.with(Component::text(p.text.clone()).styled(TextStyle {
            bold: p.bold.then_some(true),
            font: p.font.as_deref().and_then(|f| Identifier::parse(f).ok()),
            ..Default::default()
        }))
    })
}

fn probe_case(ui: &mut Ui, parts: &[Part], wrap: i32) -> Value {
    let mut cells = Vec::new();
    for p in parts {
        let mut base = Style::default();
        base.flags.set(Flags::BOLD, p.bold);
        if let Some(f) = &p.font {
            base.font = FontId::from_name(f);
        }
        push_legacy(&mut cells, &p.text, base);
    }

    let mut seen = HashSet::new();
    let mut advances = Vec::new();
    for c in &cells {
        if seen.insert((c.style.font, c.ch)) {
            let normal = ui.fonts_mut().advance(c.style.font, c.ch, false);
            let bold = ui.fonts_mut().advance(c.style.font, c.ch, true);
            advances.push(json!({
                "cp": cp(c.ch),
                "font": font_name(c.style.font),
                "normal": round(f64::from(normal), STEP),
                "bold": round(f64::from(bold), STEP),
            }));
        }
    }

    let raw: String = parts.iter().map(|p| p.text.as_str()).collect();
    let width = ui.text_width(&raw).ceil() as i32;
    let styled_width = round(f64::from(ui.width(&cells)), STEP);

    let chunks: Vec<Chunk> = parts
        .iter()
        .map(|p| {
            let mut base = Style::default();
            base.flags.set(Flags::BOLD, p.bold);
            if let Some(f) = &p.font {
                base.font = FontId::from_name(f);
            }
            Chunk::new(&p.text, base)
        })
        .collect();
    let lines: Vec<Value> = ui
        .split_lines(&chunks, wrap as f32)
        .into_iter()
        .map(|line| json!({"text": plain(&line.cells), "width": round(f64::from(ui.width(&line.cells)), STEP)}))
        .collect();

    let mut out = json!({
        "advances": advances,
        "width": width,
        "styled_width": styled_width,
        "lines": lines,
    });
    if CHAT_WIDTHS.contains(&i64::from(wrap)) {
        let mut hud = ChatHud::default();
        let options = ChatOptions {
            width: wrap,
            ..ChatOptions::default()
        };
        hud.add(ui, &component(parts), 0, &options);
        let chat: Vec<String> = hud.line_texts().into_iter().rev().map(|l| l.0).collect();
        out["chat"] = json!(chat);
    }
    out
}

impl Harness for FontHarness {
    fn name(&self) -> &'static str {
        "font"
    }

    fn scope(&self) -> &'static [&'static str] {
        &[
            "crates/mcr-ui/src/font.rs",
            "crates/mcr-ui/src/text.rs",
            "crates/mcr-ui/src/style.rs",
            "crates/mcr-ui/src/ui.rs",
            "crates/mcr-ui/src/assets.rs",
            "crates/mcr-ui/src/screens/chat.rs",
            "crates/mcr-assets/src",
            "crates/mcr-parity/src/font.rs",
        ]
    }

    fn count(&self) -> usize {
        CASES
    }

    fn case(&self, _id: usize, rng: &mut Rng) -> Value {
        generate(rng)
    }

    fn probe(&self, env: &Env, cases: &[Case]) -> Result<Vec<Value>> {
        let resources = mcr_ui::vanilla_resources(
            &env.client_jar,
            env.assets.as_ref().map(|(d, i)| (d.as_path(), i.as_str())),
        )?;
        let mut ui = Ui::load(&resources)?;
        cases
            .iter()
            .map(|c| {
                let (parts, wrap) = parse(c)?;
                Ok(probe_case(&mut ui, &parts, wrap))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cases_cover_the_interesting_inputs() {
        let cases = crate::generate(&FontHarness);
        let texts: Vec<String> = cases
            .iter()
            .flat_map(|c| c.case["parts"].as_array().unwrap().clone())
            .map(|p| p["text"].as_str().unwrap().to_owned())
            .collect();
        let any = |f: &dyn Fn(&str) -> bool| texts.iter().any(|t| f(t));
        assert!(any(&|t| t.contains("§l")));
        assert!(any(&|t| t.contains('\n')));
        assert!(any(&|t| t.contains("  ")));
        assert!(any(&|t| t.chars().any(|c| c as u32 > 0xFFFF)));
        assert!(any(&|t| t.ends_with('§')));
        assert!(any(&|t| t.split(' ').any(|w| w.chars().count() > 20)));
        let widths: Vec<i64> = cases
            .iter()
            .map(|c| c.case["wrap"].as_i64().unwrap())
            .collect();
        assert!(widths.iter().any(|&w| w <= 0) && widths.iter().any(|&w| w > 320));
    }

    #[test]
    fn case_parses_back() {
        let case = Case {
            id: 3,
            case: json!({"parts": [{"text": "a b", "bold": true, "font": null}], "wrap": 5}),
        };
        let (parts, wrap) = parse(&case).unwrap();
        assert_eq!(
            (parts[0].text.as_str(), parts[0].bold, wrap),
            ("a b", true, 5)
        );
        assert!(
            parse(&Case {
                id: 4,
                case: json!({})
            })
            .is_err()
        );
    }
}
