use std::ops::Range;

use mcr_assets::ResourceManager;
use mcr_nbt::Value;
use mcr_protocol::types::text::{Arg, Component, Content};
use rustc_hash::FxHashMap;

use crate::error::{Result, UiError};
use crate::style::{Cell, Flags, FontId, Style};

#[derive(Debug, Default)]
pub struct Lang(FxHashMap<String, String>);

impl Lang {
    pub fn load(resources: &ResourceManager) -> Result<Self> {
        let path = "assets/minecraft/lang/en_us.json";
        let Some(bytes) = resources.read(path)? else {
            tracing::warn!("missing {path}; translations fall back to keys");
            return Ok(Self::default());
        };
        let map = serde_json::from_slice(&bytes).map_err(|source| UiError::Json {
            path: path.into(),
            source,
        })?;
        Ok(Self(map))
    }

    pub fn from_pairs<'a>(pairs: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        Self(
            pairs
                .into_iter()
                .map(|(k, v)| (k.to_owned(), v.to_owned()))
                .collect(),
        )
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }
}

/// Decomposes `§` formatting codes; a reset restores `base`.
pub fn push_legacy(out: &mut Vec<Cell>, text: &str, base: Style) {
    let mut style = base;
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch == '§' {
            let Some(code) = chars.next() else { break };
            if code.eq_ignore_ascii_case(&'r') {
                style = base;
            } else if let Some(next) = style.apply_legacy(code) {
                style = next;
            }
        } else {
            out.push(Cell { ch, style });
        }
    }
}

pub fn parse_legacy(text: &str) -> Vec<Cell> {
    let mut out = Vec::with_capacity(text.len());
    push_legacy(&mut out, text, Style::default());
    out
}

fn merge(parent: Style, own: &mcr_protocol::types::text::Style) -> Style {
    let mut style = parent;
    if let Some(color) = own.color {
        style.color = Some(color.rgb());
    }
    if let Some(shadow) = own.shadow_color {
        style.shadow = Some(shadow as u32);
    }
    for (bit, value) in [
        (Flags::BOLD, own.bold),
        (Flags::ITALIC, own.italic),
        (Flags::UNDERLINE, own.underlined),
        (Flags::STRIKETHROUGH, own.strikethrough),
        (Flags::OBFUSCATED, own.obfuscated),
    ] {
        if let Some(on) = value {
            style.flags.set(bit, on);
        }
    }
    if let Some(font) = &own.font {
        style.font = FontId::from_name(font.as_str());
    }
    style
}

pub fn flatten(lang: &Lang, component: &Component) -> Vec<Cell> {
    let mut out = Vec::new();
    flatten_into(lang, component, Style::default(), &mut out);
    out
}

const MAX_CELLS: usize = 65_536;
const MAX_STEPS: usize = 100_000;
const MAX_DEPTH: usize = 64;

/// Server-sent components can reference arguments repeatedly, so expansion is bounded by work and output.
struct Budget {
    steps: usize,
    depth: usize,
}

impl Budget {
    fn enter(&mut self, out: &[Cell]) -> bool {
        if self.steps == 0 || self.depth >= MAX_DEPTH || out.len() >= MAX_CELLS {
            return false;
        }
        self.steps -= 1;
        self.depth += 1;
        true
    }

    fn charge(&mut self, bytes: usize, out: &[Cell]) -> bool {
        let cost = 1 + bytes / 16;
        if self.steps < cost || out.len() >= MAX_CELLS {
            return false;
        }
        self.steps -= cost;
        true
    }
}

pub fn flatten_into(lang: &Lang, component: &Component, parent: Style, out: &mut Vec<Cell>) {
    let mut budget = Budget {
        steps: MAX_STEPS,
        depth: 0,
    };
    flatten_bounded(lang, component, parent, out, &mut budget);
    out.truncate(MAX_CELLS);
}

fn flatten_bounded(
    lang: &Lang,
    component: &Component,
    parent: Style,
    out: &mut Vec<Cell>,
    budget: &mut Budget,
) {
    if !budget.enter(out) {
        return;
    }
    let style = merge(parent, &component.style);
    match &component.content {
        Content::Text(text) => {
            if budget.charge(text.len(), out) {
                push_legacy(out, text, style);
            }
        }
        Content::Translatable {
            key,
            fallback,
            args,
        } => {
            let template = lang
                .get(key)
                .or(fallback.as_deref())
                .unwrap_or(key.as_str());
            expand_template(lang, template, args, style, out, budget);
        }
        Content::Keybind(key) => push_legacy(out, key, style),
        Content::Score { name, .. } => push_legacy(out, name, style),
        Content::Selector { selector, .. } => push_legacy(out, selector, style),
        Content::Nbt(nbt) => push_legacy(out, &nbt.path, style),
        Content::Object { fallback, .. } => {
            if let Some(fallback) = fallback {
                flatten_bounded(lang, fallback, style, out, budget);
            }
        }
    }
    for extra in &component.extra {
        flatten_bounded(lang, extra, style, out, budget);
    }
    budget.depth -= 1;
}

fn expand_template(
    lang: &Lang,
    template: &str,
    args: &[Arg],
    style: Style,
    out: &mut Vec<Cell>,
    budget: &mut Budget,
) {
    if !budget.charge(template.len(), out) {
        return;
    }
    let mut literal = String::new();
    let mut next_arg = 0usize;
    let mut rest = template;
    while let Some(pos) = rest.find('%') {
        literal.push_str(&rest[..pos]);
        let spec = &rest[pos + 1..];
        let digits = spec.bytes().take_while(u8::is_ascii_digit).count();
        let (index, after) = if digits > 0 && spec[digits..].starts_with('$') {
            (spec[..digits].parse::<usize>().ok(), &spec[digits + 1..])
        } else {
            (None, spec)
        };
        match after.chars().next() {
            Some('%') => {
                literal.push('%');
                rest = &after[1..];
            }
            Some(c @ ('s' | 'd')) => {
                push_legacy(out, &std::mem::take(&mut literal), style);
                let slot = match index {
                    Some(n) => n.saturating_sub(1),
                    None => {
                        next_arg += 1;
                        next_arg - 1
                    }
                };
                if let Some(arg) = args.get(slot) {
                    if budget.steps == 0 || out.len() >= MAX_CELLS {
                        return;
                    }
                    budget.steps -= 1;
                    push_arg(lang, arg, style, out, budget);
                }
                rest = &after[c.len_utf8()..];
            }
            _ => {
                literal.push('%');
                rest = spec;
            }
        }
    }
    literal.push_str(rest);
    push_legacy(out, &literal, style);
}

fn push_arg(lang: &Lang, arg: &Arg, style: Style, out: &mut Vec<Cell>, budget: &mut Budget) {
    match arg {
        Arg::Str(s) => {
            if budget.charge(s.len(), out) {
                push_legacy(out, s, style);
            }
        }
        Arg::Number(n) => push_legacy(out, &number_text(n), style),
        Arg::Component(c) => flatten_bounded(lang, c, style, out, budget),
    }
}

fn number_text(value: &Value) -> String {
    let float = |f: f64| {
        if f.fract() == 0.0 {
            format!("{f:.1}")
        } else {
            f.to_string()
        }
    };
    match value {
        Value::Byte(v) => v.to_string(),
        Value::Short(v) => v.to_string(),
        Value::Int(v) => v.to_string(),
        Value::Long(v) => v.to_string(),
        Value::Float(v) => float(f64::from(*v)),
        Value::Double(v) => float(*v),
        Value::String(s) => s.clone(),
        _ => String::new(),
    }
}

/// Port of vanilla `StringSplitter.splitLines`: break at the last space on overflow, else at the
/// overflowing cell; `\n` always breaks and every line keeps at least one cell.
pub fn wrap(
    cells: &[Cell],
    max_width: f32,
    mut width_of: impl FnMut(&Cell) -> f32,
) -> Vec<Range<usize>> {
    let max_width = max_width.max(1.0);
    let mut lines = Vec::new();
    let mut start = 0;
    let mut forced_empty_tail = false;
    while start < cells.len() {
        let mut width = 0.0;
        let mut seen_width = false;
        let mut last_space = None;
        let mut split = None;
        for (i, cell) in cells.iter().enumerate().skip(start) {
            if cell.ch == '\n' {
                split = Some(i);
                break;
            }
            if cell.ch == ' ' {
                last_space = Some(i);
            }
            let w = width_of(cell);
            width += w;
            if seen_width && width > max_width {
                split = Some(last_space.unwrap_or(i));
                break;
            }
            seen_width |= w != 0.0;
        }
        let Some(at) = split else {
            lines.push(start..cells.len());
            return lines;
        };
        lines.push(start..at);
        let skip = matches!(cells[at].ch, '\n' | ' ');
        forced_empty_tail = cells[at].ch == '\n';
        start = at + usize::from(skip);
    }
    if forced_empty_tail {
        lines.push(cells.len()..cells.len());
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcr_protocol::types::text::{Color, NamedColor, Style as ProtoStyle};

    fn plain(cells: &[Cell]) -> String {
        cells.iter().map(|c| c.ch).collect()
    }

    fn lines(text: &str, width: f32) -> Vec<String> {
        let cells = parse_legacy(text);
        wrap(&cells, width, |_| 1.0)
            .into_iter()
            .map(|r| plain(&cells[r]))
            .collect()
    }

    #[test]
    fn legacy_codes_color_reset_and_are_invisible() {
        let cells = parse_legacy("§cRed§r plain§");
        assert_eq!(plain(&cells), "Red plain");
        assert_eq!(cells[0].style.color, Some(0xFF5555));
        assert_eq!(cells[3].style.color, None);
    }

    #[test]
    fn legacy_color_clears_formatting_but_formatting_stacks() {
        let cells = parse_legacy("§l§oA§aB");
        assert!(cells[0].style.bold() && cells[0].style.italic());
        assert!(!cells[1].style.bold());
        assert_eq!(cells[1].style.color, Some(0x55FF55));
    }

    #[test]
    fn unknown_code_is_swallowed() {
        assert_eq!(plain(&parse_legacy("a§zb")), "ab");
    }

    #[test]
    fn hostile_argument_expansion_is_bounded() {
        let lang = Lang::from_pairs([("x", "%1$s%1$s%1$s%1$s%1$s%1$s%1$s%1$s")]);
        let mut c = Component::text("ab");
        for _ in 0..40 {
            c = Component::from_content(Content::Translatable {
                key: "x".into(),
                fallback: None,
                args: vec![Arg::Component(c)],
            });
        }
        let start = std::time::Instant::now();
        let cells = flatten(&lang, &c);
        assert!(cells.len() <= MAX_CELLS);
        assert!(start.elapsed() < std::time::Duration::from_secs(1));

        let long = format!("{}{}", "%s".repeat(10_000), "x".repeat(10_000));
        let mut c = Component::text("ab");
        for _ in 0..8 {
            c = Component::from_content(Content::Translatable {
                key: "missing".into(),
                fallback: Some(long.clone()),
                args: vec![Arg::Component(c)],
            });
        }
        let start = std::time::Instant::now();
        assert!(flatten(&lang, &c).len() <= MAX_CELLS);
        assert!(start.elapsed() < std::time::Duration::from_secs(1));
    }

    #[test]
    fn translation_positional_and_percent_escape() {
        let lang = Lang::from_pairs([
            ("chat.type.text", "<%s> %s"),
            ("t.swap", "%2$s then %1$s, 100%%"),
        ]);
        let say = Component::from_content(Content::Translatable {
            key: "chat.type.text".into(),
            fallback: None,
            args: vec![Arg::Str("Steve".into()), Arg::Str("hi".into())],
        });
        assert_eq!(plain(&flatten(&lang, &say)), "<Steve> hi");
        let swap = Component::from_content(Content::Translatable {
            key: "t.swap".into(),
            fallback: None,
            args: vec![Arg::Str("a".into()), Arg::Str("b".into())],
        });
        assert_eq!(plain(&flatten(&lang, &swap)), "b then a, 100%");
    }

    #[test]
    fn missing_key_uses_fallback_then_key() {
        let lang = Lang::default();
        let with = Component::from_content(Content::Translatable {
            key: "nope".into(),
            fallback: Some("fb".into()),
            args: vec![],
        });
        assert_eq!(plain(&flatten(&lang, &with)), "fb");
        let without = Component::from_content(Content::Translatable {
            key: "nope".into(),
            fallback: None,
            args: vec![],
        });
        assert_eq!(plain(&flatten(&lang, &without)), "nope");
    }

    #[test]
    fn style_inherits_to_children_and_arguments() {
        let red = ProtoStyle {
            color: Some(Color::Named(NamedColor::Red)),
            bold: Some(true),
            ..Default::default()
        };
        let child = Component::text("b").styled(ProtoStyle {
            bold: Some(false),
            color: Some(Color::Rgb(0x123456)),
            ..Default::default()
        });
        let root = Component::text("a").styled(red).with(child);
        let cells = flatten(&Lang::default(), &root);
        assert_eq!(cells[0].style.color, Some(0xFF5555));
        assert!(cells[0].style.bold());
        assert_eq!(cells[1].style.color, Some(0x123456));
        assert!(!cells[1].style.bold());
    }

    #[test]
    fn wrap_breaks_at_last_space() {
        assert_eq!(lines("aaa bbb ccc", 7.0), ["aaa bbb", "ccc"]);
    }

    #[test]
    fn wrap_splits_long_word_at_overflow() {
        assert_eq!(lines("abcdefghij", 4.0), ["abcd", "efgh", "ij"]);
    }

    #[test]
    fn wrap_honours_newlines_and_trailing_newline() {
        assert_eq!(lines("ab\ncd", 10.0), ["ab", "cd"]);
        assert_eq!(lines("ab\n", 10.0), ["ab", ""]);
        assert_eq!(lines("\nab", 10.0), ["", "ab"]);
        assert!(lines("", 10.0).is_empty());
    }

    #[test]
    fn wrap_keeps_one_cell_per_line_when_width_is_tiny() {
        assert_eq!(lines("abc", 0.5), ["a", "b", "c"]);
    }

    #[test]
    fn wrap_drops_the_space_at_the_break_only() {
        assert_eq!(lines("aaaa  bb", 5.0), ["aaaa ", "bb"]);
    }
}
