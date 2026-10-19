use std::ops::Range;

use mcr_nbt::Value;
use mcr_protocol::types::text::{Arg, ClickEvent, Component, Content};

pub use crate::lang::Lang;
use crate::style::{Cell, Flags, FontId, Style};

/// Decomposes `§` formatting codes; a reset restores `base`.
pub fn push_legacy(out: &mut Vec<Cell>, text: &str, base: Style) {
    push_formatted(out, text, base, false);
}

/// `push_legacy`, or with `strip` the `chatColors` off path that drops the codes without applying them.
fn push_formatted(out: &mut Vec<Cell>, text: &str, base: Style, strip: bool) {
    let mut style = base;
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch == '§' {
            let Some(code) = chars.next() else { break };
            if strip {
                continue;
            }
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

/// Cells covered by a component carrying a click event; inner spans come before the spans enclosing them.
#[derive(Clone, Debug, PartialEq)]
pub struct ClickSpan {
    pub range: Range<usize>,
    pub event: ClickEvent,
}

/// Cells covered by a component with a Shift-click insertion.
#[derive(Clone, Debug, PartialEq)]
pub struct InsertSpan {
    pub range: Range<usize>,
    pub text: String,
}

const MAX_SPANS: usize = 256;
const MAX_INSERTION: usize = 256;

pub fn flatten_events(lang: &Lang, component: &Component) -> (Vec<Cell>, Vec<ClickSpan>) {
    let (cells, spans, _) = flatten_chat(lang, component, true);
    (cells, spans)
}

/// A chat message's cells, click spans and insertion spans; `colors` false strips `§` codes like
/// vanilla's `chatColors` option.
pub fn flatten_chat(
    lang: &Lang,
    component: &Component,
    colors: bool,
) -> (Vec<Cell>, Vec<ClickSpan>, Vec<InsertSpan>) {
    let mut out = Vec::new();
    let mut budget = Budget {
        steps: MAX_STEPS,
        depth: 0,
        spans: Some(Vec::new()),
        inserts: Some(Vec::new()),
        strip: !colors,
    };
    flatten_bounded(lang, component, Style::default(), &mut out, &mut budget);
    out.truncate(MAX_CELLS);
    let mut spans = budget.spans.unwrap_or_default();
    spans.retain(|s| s.range.start < out.len());
    let mut inserts = budget.inserts.unwrap_or_default();
    inserts.retain(|s| s.range.start < out.len());
    (out, spans, inserts)
}

/// The innermost click event covering `index`.
pub fn click_at(spans: &[ClickSpan], index: usize) -> Option<&ClickEvent> {
    spans
        .iter()
        .filter(|s| s.range.contains(&index))
        .min_by_key(|s| s.range.len())
        .map(|s| &s.event)
}

const MAX_CELLS: usize = 65_536;
const MAX_STEPS: usize = 100_000;
const MAX_DEPTH: usize = 64;

/// Server-sent components can reference arguments repeatedly, so expansion is bounded by work and output.
struct Budget {
    steps: usize,
    depth: usize,
    spans: Option<Vec<ClickSpan>>,
    inserts: Option<Vec<InsertSpan>>,
    strip: bool,
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
        spans: None,
        inserts: None,
        strip: false,
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
    let start = out.len();
    match &component.content {
        Content::Text(text) => {
            if budget.charge(text.len(), out) {
                push_formatted(out, text, style, budget.strip);
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
        Content::Keybind(key) => push_formatted(out, key, style, budget.strip),
        Content::Score { name, .. } => push_formatted(out, name, style, budget.strip),
        Content::Selector { selector, .. } => {
            push_formatted(out, selector, style, budget.strip);
        }
        Content::Nbt(nbt) => push_formatted(out, &nbt.path, style, budget.strip),
        Content::Object { fallback, .. } => {
            if let Some(fallback) = fallback {
                flatten_bounded(lang, fallback, style, out, budget);
            }
        }
    }
    for extra in &component.extra {
        flatten_bounded(lang, extra, style, out, budget);
    }
    if let (Some(spans), Some(event)) = (&mut budget.spans, &component.style.click_event)
        && spans.len() < MAX_SPANS
        && out.len() > start
    {
        spans.push(ClickSpan {
            range: start..out.len(),
            event: (**event).clone(),
        });
    }
    if let (Some(inserts), Some(text)) = (&mut budget.inserts, &component.style.insertion)
        && inserts.len() < MAX_SPANS
        && out.len() > start
        && text.chars().count() <= MAX_INSERTION
    {
        inserts.push(InsertSpan {
            range: start..out.len(),
            text: text.clone(),
        });
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
                push_formatted(out, &std::mem::take(&mut literal), style, budget.strip);
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
    push_formatted(out, &literal, style, budget.strip);
}

fn push_arg(lang: &Lang, arg: &Arg, style: Style, out: &mut Vec<Cell>, budget: &mut Budget) {
    match arg {
        Arg::Str(s) => {
            if budget.charge(s.len(), out) {
                push_formatted(out, s, style, budget.strip);
            }
        }
        Arg::Number(n) => push_formatted(out, &number_text(n), style, budget.strip),
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

    #[test]
    fn click_spans_cover_their_cells_and_nest() {
        use mcr_protocol::types::text::{ClickEvent, Style as ProtoStyle};
        let on = |event| ProtoStyle {
            click_event: Some(Box::new(event)),
            ..Default::default()
        };
        let inner = Component::text("bc").styled(on(ClickEvent::RunCommand("/in".into())));
        let outer = Component::text("a")
            .with(inner)
            .with(Component::text("d"))
            .styled(on(ClickEvent::OpenUrl("https://x".into())));
        let (cells, spans) = flatten_events(&Lang::default(), &Component::text("-").with(outer));
        assert_eq!(plain(&cells), "-abcd");
        assert_eq!(click_at(&spans, 0), None);
        assert_eq!(
            click_at(&spans, 1),
            Some(&ClickEvent::OpenUrl("https://x".into()))
        );
        assert_eq!(
            click_at(&spans, 2),
            Some(&ClickEvent::RunCommand("/in".into()))
        );
        assert_eq!(
            click_at(&spans, 3),
            Some(&ClickEvent::RunCommand("/in".into()))
        );
        assert_eq!(
            click_at(&spans, 4),
            Some(&ClickEvent::OpenUrl("https://x".into()))
        );
        assert_eq!(click_at(&spans, 5), None);
    }

    #[test]
    fn click_spans_are_bounded() {
        use mcr_protocol::types::text::{ClickEvent, Style as ProtoStyle};
        let style = ProtoStyle {
            click_event: Some(Box::new(ClickEvent::CopyToClipboard("x".into()))),
            ..Default::default()
        };
        let mut root = Component::text("");
        for _ in 0..2000 {
            root = root.with(Component::text("y").styled(style.clone()));
        }
        let (_, spans) = flatten_events(&Lang::default(), &root);
        assert!(spans.len() <= 256);
    }

    #[test]
    fn chat_colors_off_strips_legacy_codes_but_keeps_styles() {
        let message = Component::text("§cred").with(Component::text("blue").styled(
            mcr_protocol::types::text::Style {
                color: Some(mcr_protocol::types::text::Color::Named(
                    mcr_protocol::types::text::NamedColor::Blue,
                )),
                insertion: Some("ins".into()),
                ..Default::default()
            },
        ));
        let (on, _, inserts) = flatten_chat(&Lang::default(), &message, true);
        let (off, _, _) = flatten_chat(&Lang::default(), &message, false);
        let text = |cells: &[Cell]| cells.iter().map(|c| c.ch).collect::<String>();
        assert_eq!(text(&on), "redblue");
        assert_eq!(text(&off), "redblue");
        assert_eq!(on[0].style.color, Some(0xFF_5555));
        assert_eq!(off[0].style.color, None);
        assert_eq!(off[3].style.color, Some(0x55_55FF));
        assert_eq!(
            inserts,
            [InsertSpan {
                range: 3..7,
                text: "ins".into()
            }]
        );
    }
}
