use std::borrow::Cow;
use std::ops::Range;

use mcr_protocol::types::text::{
    Arg, ClickEvent, Component, Content, TemplatePiece, decompose_template, number_to_string,
};

pub use crate::lang::Lang;
use crate::style::{Cell, Flags, FontId, Style};

/// Decomposes `§` formatting codes; a reset restores `base`.
pub fn push_legacy(out: &mut Vec<Cell>, text: &str, base: Style) {
    push_prepared(out, &normalize(text), base);
}

/// `StringDecomposer#iterateFormatted`: feeds `(byte position, char, style)` to `sink` until it returns
/// false. `§r` restores `reset`; an unknown code and a trailing `§` change nothing.
fn decompose(
    text: &str,
    mut style: Style,
    reset: Style,
    mut sink: impl FnMut(usize, char, Style) -> bool,
) -> bool {
    let mut chars = text.char_indices();
    while let Some((i, ch)) = chars.next() {
        if ch != '§' {
            if !sink(i, ch, style) {
                return false;
            }
            continue;
        }
        let Some((_, code)) = chars.next() else { break };
        if code.eq_ignore_ascii_case(&'r') {
            style = reset;
        } else if let Some(next) = style.apply_legacy(code) {
            style = next;
        }
    }
    true
}

/// Vanilla reads a code as one UTF-16 unit, so `§` before a supplementary character swallows its high
/// half and leaves a replacement character for the low half.
fn normalize(text: &str) -> Cow<'_, str> {
    let Some(first) = text.find('§') else {
        return Cow::Borrowed(text);
    };
    let supplementary = |s: &str| {
        s.chars()
            .zip(s.chars().skip(1))
            .any(|(a, b)| a == '§' && b as u32 > 0xFFFF)
    };
    if !supplementary(&text[first..]) {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        out.push(ch);
        if ch != '§' {
            continue;
        }
        match chars.next() {
            Some(code) if code as u32 > 0xFFFF => {
                out.replace_range(out.len() - '§'.len_utf8().., "\u{FFFD}")
            }
            Some(code) => out.push(code),
            None => {}
        }
    }
    Cow::Owned(out)
}

/// `ChatFormatting#stripFormatting`: drops `§` followed by a formatting code.
fn strip_formatting(text: &str) -> Cow<'_, str> {
    if !text.contains('§') {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '§'
            && chars
                .peek()
                .is_some_and(|c| c.is_ascii_hexdigit() || "klmnorKLMNOR".contains(*c))
        {
            chars.next();
        } else {
            out.push(ch);
        }
    }
    Cow::Owned(out)
}

fn prepare(text: &str, strip: bool) -> Cow<'_, str> {
    match (strip, normalize(text)) {
        (false, text) => text,
        (true, Cow::Borrowed(text)) => strip_formatting(text),
        (true, Cow::Owned(text)) => Cow::Owned(strip_formatting(&text).into_owned()),
    }
}

fn push_prepared(out: &mut Vec<Cell>, text: &str, base: Style) {
    decompose(text, base, base, |_, ch, style| {
        out.push(Cell { ch, style });
        true
    });
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
    let chat = flatten_chat(lang, component, true);
    (chat.cells, chat.spans)
}

/// A chat message flattened: its cells, the text pieces they came from, and the spans over the cells.
#[derive(Clone, Debug, Default)]
pub struct ChatText {
    pub cells: Vec<Cell>,
    pub chunks: Vec<Chunk>,
    pub spans: Vec<ClickSpan>,
    pub inserts: Vec<InsertSpan>,
}

/// A chat message's cells, text pieces, click spans and insertion spans; `colors` false strips `§`
/// codes like vanilla's `chatColors` option.
pub fn flatten_chat(lang: &Lang, component: &Component, colors: bool) -> ChatText {
    let mut out = Vec::new();
    let mut budget = Budget {
        steps: MAX_STEPS,
        depth: 0,
        spans: Some(Vec::new()),
        inserts: Some(Vec::new()),
        chunks: Some(Vec::new()),
        strip: !colors,
    };
    flatten_bounded(lang, component, Style::default(), &mut out, &mut budget);
    out.truncate(MAX_CELLS);
    let mut spans = budget.spans.unwrap_or_default();
    spans.retain(|s| s.range.start < out.len());
    let mut inserts = budget.inserts.unwrap_or_default();
    inserts.retain(|s| s.range.start < out.len());
    ChatText {
        cells: out,
        chunks: budget.chunks.unwrap_or_default(),
        spans,
        inserts,
    }
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
    chunks: Option<Vec<Chunk>>,
    strip: bool,
}

impl Budget {
    fn push(&mut self, out: &mut Vec<Cell>, text: &str, style: Style) {
        let text = prepare(text, self.strip);
        if let Some(chunks) = &mut self.chunks
            && !text.is_empty()
        {
            chunks.push(Chunk::raw(text.clone().into_owned(), style));
        }
        push_prepared(out, &text, style);
    }

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
        chunks: None,
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
                budget.push(out, text, style);
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
        Content::Keybind(key) => budget.push(out, key, style),
        Content::Score { .. } | Content::Nbt(_) => {}
        Content::Selector { selector, .. } => {
            budget.push(out, selector, style);
        }
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
    let Some(pieces) = decompose_template(template, args.len()) else {
        budget.push(out, template, style);
        return;
    };
    for piece in pieces {
        match piece {
            TemplatePiece::Literal(text) => budget.push(out, text, style),
            TemplatePiece::Percent => budget.push(out, "%", style),
            TemplatePiece::Arg(slot) => {
                if budget.steps == 0 || out.len() >= MAX_CELLS {
                    return;
                }
                budget.steps -= 1;
                push_arg(lang, &args[slot], style, out, budget);
            }
        }
    }
}

fn push_arg(lang: &Lang, arg: &Arg, style: Style, out: &mut Vec<Cell>, budget: &mut Budget) {
    match arg {
        Arg::Str(s) => {
            if budget.charge(s.len(), out) {
                budget.push(out, s, style);
            }
        }
        Arg::Number(n) => budget.push(out, &number_to_string(n), style),
        Arg::Component(c) => flatten_bounded(lang, c, style, out, budget),
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

/// A piece of message text with the style it starts in, `§` codes still inside: vanilla's wrapping
/// treats a reset differently from how the piece is drawn, so it needs the raw text.
#[derive(Clone, Debug, PartialEq)]
pub struct Chunk {
    text: String,
    pub style: Style,
}

impl Chunk {
    pub fn new(text: &str, style: Style) -> Self {
        Self::raw(normalize(text).into_owned(), style)
    }

    fn raw(text: String, style: Style) -> Self {
        Self { text, style }
    }
}

/// A line out of [`split_lines`]: its cells styled as vanilla draws them, the index of its first cell among
/// all the chunks' cells, and whether it continues a line broken for width.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub start: usize,
    pub cells: Vec<Cell>,
    pub wrapped: bool,
}

struct Piece<'a> {
    text: &'a str,
    style: Style,
}

#[derive(Clone, Copy)]
struct Break {
    piece: usize,
    at: usize,
    style: Style,
    ch: char,
}

struct BreakFinder {
    max_width: f32,
    width: f32,
    seen_width: bool,
    last_space: Option<Break>,
    found: Option<Break>,
}

impl BreakFinder {
    fn new(max_width: f32) -> Self {
        Self {
            max_width: max_width.max(1.0),
            width: 0.0,
            seen_width: false,
            last_space: None,
            found: None,
        }
    }

    fn accept(&mut self, piece: usize, at: usize, ch: char, style: Style, w: f32) -> bool {
        let here = Break {
            piece,
            at,
            style,
            ch,
        };
        if ch == '\n' {
            self.found = Some(here);
            return false;
        }
        if ch == ' ' {
            self.last_space = Some(here);
        }
        self.width += w;
        if self.seen_width && self.width > self.max_width {
            self.found = Some(self.last_space.unwrap_or(here));
            return false;
        }
        self.seen_width |= w != 0.0;
        true
    }
}

fn piece_cells(piece: &str, style: Style, out: &mut Vec<Cell>) {
    push_prepared(out, piece, style);
}

/// `StringSplitter#splitLines` over styled text. Breaks are found with `§r` resetting to the plain style
/// and each line's remainder starts in the style active at its break, as vanilla does.
pub fn split_lines(
    chunks: &[Chunk],
    max_width: f32,
    mut width_of: impl FnMut(&Cell) -> f32,
) -> Vec<Line> {
    let mut pieces: Vec<Piece> = chunks
        .iter()
        .filter(|c| !c.text.is_empty())
        .map(|c| Piece {
            text: &c.text,
            style: c.style,
        })
        .collect();
    let mut lines = Vec::new();
    let mut first = 0;
    let mut start = 0;
    let mut wrapped = false;
    let mut forced_new_line = false;
    while first < pieces.len() {
        let mut finder = BreakFinder::new(max_width);
        for (k, piece) in pieces.iter().enumerate().skip(first) {
            let finished = decompose(
                piece.text,
                piece.style,
                Style::default(),
                |at, ch, style| {
                    let w = width_of(&Cell { ch, style });
                    finder.accept(k, at, ch, style, w)
                },
            );
            if !finished {
                break;
            }
        }
        let Some(split) = finder.found else { break };
        let mut cells = Vec::new();
        for piece in &pieces[first..split.piece] {
            piece_cells(piece.text, piece.style, &mut cells);
        }
        let cut = &pieces[split.piece];
        piece_cells(&cut.text[..split.at], cut.style, &mut cells);
        let skip = matches!(split.ch, '\n' | ' ');
        let skipped = usize::from(skip);
        let count = cells.len();
        lines.push(Line {
            start,
            cells,
            wrapped,
        });
        start += count + skipped;
        wrapped = split.ch != '\n';
        forced_new_line = split.ch == '\n';
        let rest = &cut.text[split.at + skipped..];
        pieces[split.piece] = Piece {
            text: rest,
            style: split.style,
        };
        first = split.piece + usize::from(rest.is_empty());
    }
    if first < pieces.len() {
        let mut cells = Vec::new();
        for piece in &pieces[first..] {
            piece_cells(piece.text, piece.style, &mut cells);
        }
        lines.push(Line {
            start,
            cells,
            wrapped,
        });
    } else if forced_new_line {
        lines.push(Line {
            start,
            cells: Vec::new(),
            wrapped: false,
        });
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
    fn invalid_templates_draw_raw_and_percent_is_its_own_piece() {
        let lang = Lang::default();
        let tr = |fallback: &str, args| {
            Component::from_content(Content::Translatable {
                key: "k".into(),
                fallback: Some(fallback.into()),
                args,
            })
        };
        assert_eq!(
            plain(&flatten(&lang, &tr("%0$s %d", vec![Arg::Str("a".into())]))),
            "%0$s %d"
        );
        assert_eq!(plain(&flatten(&lang, &tr("%s %s", vec![]))), "%s %s");
        let cells = flatten(&lang, &tr("§c%%a", vec![]));
        assert_eq!(plain(&cells), "%a");
        assert_eq!(cells[0].style.color, None);
    }

    #[test]
    fn score_and_nbt_draw_nothing() {
        let score = Component::from_content(Content::Score {
            name: "@s".into(),
            objective: "o".into(),
        });
        assert!(flatten(&Lang::default(), &score).is_empty());
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
        let ChatText {
            cells: on, inserts, ..
        } = flatten_chat(&Lang::default(), &message, true);
        let off = flatten_chat(&Lang::default(), &message, false).cells;
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

    fn split(chunks: &[(&str, Style)], width: f32) -> Vec<(String, bool)> {
        let chunks: Vec<Chunk> = chunks.iter().map(|(t, s)| Chunk::new(t, *s)).collect();
        split_lines(&chunks, width, |_| 1.0)
            .into_iter()
            .map(|l| (plain(&l.cells), l.wrapped))
            .collect()
    }

    #[test]
    fn split_lines_flags_wrapped_continuations_only() {
        let lines = split(&[("aaa bbb\nccc", Style::default())], 3.0);
        assert_eq!(
            lines,
            [
                ("aaa".into(), false),
                ("bbb".into(), true),
                ("ccc".into(), false)
            ]
        );
    }

    #[test]
    fn split_lines_keeps_a_final_line_for_trailing_codes() {
        let lines = split(&[("ab §l", Style::default())], 2.0);
        assert_eq!(lines, [("ab".into(), false), (String::new(), true)]);
        assert_eq!(
            split(&[("§l", Style::default())], 5.0),
            [(String::new(), false)]
        );
        assert_eq!(split(&[("ab\n", Style::default())], 5.0).len(), 2);
        assert!(split(&[("", Style::default())], 5.0).is_empty());
    }

    #[test]
    fn split_lines_resets_to_the_style_at_the_break() {
        let chunks = [Chunk::new("§lab c§rd", Style::default())];
        let lines = split_lines(&chunks, 3.0, |_| 1.0);
        assert_eq!(lines.len(), 2);
        assert!(lines[1].cells.iter().all(|c| c.style.bold()));
    }

    #[test]
    fn section_sign_before_a_supplementary_char_leaves_a_replacement() {
        assert_eq!(plain(&parse_legacy("a§\u{1F600}b")), "a\u{FFFD}b");
        assert_eq!(plain(&parse_legacy("§§\u{1F600}")), "\u{1F600}");
    }

    #[test]
    fn unknown_fonts_are_missing_but_keep_their_name() {
        let font = FontId::from_name("minecraft:nonexistent");
        assert!(font.is_missing());
        assert_eq!(font.path(), "nonexistent");
        assert_eq!(FontId::from_name("nonexistent"), font);
        assert_eq!(FontId::from_name("minecraft:alt"), FontId::ALT);
    }
}
