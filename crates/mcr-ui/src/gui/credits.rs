//! The End poem and credits (`WinScreen`): scrolling text over a dark backdrop.

use super::env::{Env, RenderCx};
use super::input::{Key, UiInput};
use super::screen::{Screen, ScreenAction};
use crate::style::Cell;
use crate::text::parse_legacy;

pub const POEM_PATH: &str = "assets/minecraft/texts/end.txt";
pub const CREDITS_PATH: &str = "assets/minecraft/texts/credits.json";
pub const POSTCREDITS_PATH: &str = "assets/minecraft/texts/postcredits.txt";

const LINE_HEIGHT: i32 = 12;
const COLUMN_WIDTH: f32 = 256.0;
const POEM_SPEED: f32 = 0.5;
const CREDITS_SPEED: f32 = 0.75;
const SPEEDUP: f32 = 5.0;
const MAX_LINES: usize = 20_000;
const TRAILING_BLANKS: usize = 8;
const OBFUSCATE_TOKEN: &str = "§f§k§a§b";
const OBFUSCATED_PREFIX: &str = "§f§k";
const SECTION_HEADING: &str = "============";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreditsLine {
    pub text: String,
    pub centered: bool,
    pub wrap: bool,
}

impl CreditsLine {
    fn blank() -> Self {
        Self {
            text: String::new(),
            centered: false,
            wrap: false,
        }
    }
}

/// Raw files of the poem and credits as the client jar ships them.
#[derive(Clone, Debug, Default)]
pub struct CreditsSource {
    pub poem: Option<String>,
    pub credits: Option<String>,
    pub postcredits: Option<String>,
}

fn push(lines: &mut Vec<CreditsLine>, line: CreditsLine) {
    if lines.len() < MAX_LINES {
        lines.push(line);
    }
}

fn poem_lines(text: &str, player: &str, lines: &mut Vec<CreditsLine>) {
    let mut seed = 8_124_371u32;
    for raw in text.lines() {
        let mut line = raw.replace("PLAYERNAME", player);
        while let Some(at) = line.find(OBFUSCATE_TOKEN) {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let length = 3 + (seed >> 16) as usize % 4;
            let mask = format!("{OBFUSCATED_PREFIX}{}", "X".repeat(length));
            line.replace_range(at..at + OBFUSCATE_TOKEN.len(), &mask);
        }
        push(
            lines,
            CreditsLine {
                text: line,
                centered: false,
                wrap: true,
            },
        );
        push(lines, CreditsLine::blank());
    }
    for _ in 0..TRAILING_BLANKS {
        push(lines, CreditsLine::blank());
    }
}

fn credits_lines(json: &str, lines: &mut Vec<CreditsLine>) {
    let Ok(serde_json::Value::Array(sections)) = serde_json::from_str(json) else {
        return;
    };
    let centered = |text: String| CreditsLine {
        text,
        centered: true,
        wrap: false,
    };
    let left = |text: String| CreditsLine {
        text,
        centered: false,
        wrap: false,
    };
    let str_of = |v: &serde_json::Value, key: &str| {
        v.get(key)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    for section in &sections {
        push(lines, centered(format!("§f{SECTION_HEADING}")));
        push(lines, centered(format!("§e{}", str_of(section, "section"))));
        push(lines, centered(format!("§f{SECTION_HEADING}")));
        push(lines, CreditsLine::blank());
        push(lines, CreditsLine::blank());
        let disciplines = section.get("disciplines").and_then(|d| d.as_array());
        for discipline in disciplines.into_iter().flatten() {
            let name = str_of(discipline, "discipline");
            if !name.is_empty() {
                push(lines, centered(format!("§e{name}")));
                push(lines, CreditsLine::blank());
                push(lines, CreditsLine::blank());
            }
            let titles = discipline.get("titles").and_then(|t| t.as_array());
            for title in titles.into_iter().flatten() {
                push(lines, left(format!("§7{}", str_of(title, "title"))));
                let names = title.get("names").and_then(|n| n.as_array());
                for name in names.into_iter().flatten().filter_map(|n| n.as_str()) {
                    push(lines, left(format!("§f           {name}")));
                }
                push(lines, CreditsLine::blank());
                push(lines, CreditsLine::blank());
            }
        }
    }
}

/// Poem (End only), then credits, then the post-credits text.
pub fn build_lines(source: &CreditsSource, poem: bool, player: &str) -> Vec<CreditsLine> {
    let mut lines = Vec::new();
    if poem && let Some(text) = &source.poem {
        poem_lines(text, player, &mut lines);
    }
    if let Some(json) = &source.credits {
        credits_lines(json, &mut lines);
    }
    if poem && let Some(text) = &source.postcredits {
        poem_lines(text, player, &mut lines);
    }
    lines
}

/// Scroll position and speed, separate from drawing so the lifecycle is testable.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scroll {
    pub offset: f32,
    base_speed: f32,
    reverse: bool,
    fast: bool,
}

impl Scroll {
    pub fn new(poem: bool) -> Self {
        Self {
            offset: 0.0,
            base_speed: if poem { POEM_SPEED } else { CREDITS_SPEED },
            reverse: false,
            fast: false,
        }
    }

    pub fn speed(&self) -> f32 {
        let factor = if self.fast { SPEEDUP } else { 1.0 };
        self.base_speed * factor * if self.reverse { -1.0 } else { 1.0 }
    }

    pub fn tick(&mut self) {
        self.offset = (self.offset + self.speed()).max(0.0);
    }

    pub fn finished(&self, text_height: i32, screen_height: i32) -> bool {
        self.offset > (text_height + screen_height * 2 + 24) as f32
    }
}

pub struct CreditsScreen {
    poem: bool,
    source: Vec<CreditsLine>,
    laid_out: Option<Vec<(Vec<Cell>, bool)>>,
    scroll: Scroll,
    gui: (i32, i32),
    last_tick_ms: Option<u64>,
}

impl std::fmt::Debug for CreditsScreen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CreditsScreen")
            .field("poem", &self.poem)
            .field("offset", &self.scroll.offset)
            .finish_non_exhaustive()
    }
}

impl CreditsScreen {
    pub fn new(source: &CreditsSource, poem: bool, player: &str) -> Self {
        Self {
            poem,
            source: build_lines(source, poem, player),
            laid_out: None,
            scroll: Scroll::new(poem),
            gui: (0, 0),
            last_tick_ms: None,
        }
    }

    pub fn offset(&self) -> f32 {
        self.scroll.offset
    }

    fn text_height(&self) -> i32 {
        let lines = self.laid_out.as_ref().map_or(self.source.len(), Vec::len);
        lines as i32 * LINE_HEIGHT
    }

    fn lay_out(&mut self, cx: &mut RenderCx<'_, '_>) {
        if self.laid_out.is_some() {
            return;
        }
        let mut out = Vec::with_capacity(self.source.len());
        for line in &self.source {
            let cells = parse_legacy(&line.text);
            if !line.wrap {
                out.push((cells, line.centered));
                continue;
            }
            let ranges = cx.frame.ui().wrap(&cells, COLUMN_WIDTH);
            if ranges.is_empty() {
                out.push((Vec::new(), false));
            }
            for range in ranges {
                if out.len() < MAX_LINES {
                    out.push((cells[range].to_vec(), false));
                }
            }
        }
        self.laid_out = Some(out);
    }
}

impl Screen for CreditsScreen {
    fn init(&mut self, width: i32, height: i32) {
        self.gui = (width, height);
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        self.lay_out(cx);
        let (w, h) = cx.gui_size();
        cx.frame.fill_rect(0, 0, w, h, 0xFF00_0000);
        let Some(lines) = &self.laid_out else { return };
        let ticks_behind = self.last_tick_ms.map_or(0.0, |t| {
            (cx.now_ms.saturating_sub(t) as f32 / 50.0).min(1.0)
        });
        let offset = (self.scroll.offset + self.scroll.speed() * ticks_behind).max(0.0);
        let column = w / 2 - 128;
        let top = h + 50 + 100;
        for (i, (cells, centered)) in lines.iter().enumerate() {
            let mut y = top + i as i32 * LINE_HEIGHT - offset as i32;
            if i + 1 == lines.len() {
                y = y.max(h / 2 - 6);
            }
            if y + LINE_HEIGHT > 0 && y < h && !cells.is_empty() {
                let width = cx.frame.ui().width(cells);
                let x = if *centered {
                    column as f32 + 128.0 - width / 2.0
                } else {
                    column as f32
                };
                cx.frame
                    .cells(cells, x.floor(), y as f32, 0xFFFF_FFFF, true);
            }
        }
    }

    fn input(&mut self, _env: &mut Env<'_>, input: &UiInput, out: &mut Vec<ScreenAction>) -> bool {
        let UiInput::Key { key, .. } = input else {
            return false;
        };
        match key {
            Key::Escape => out.push(ScreenAction::Respawn),
            Key::Up => self.scroll.reverse = true,
            Key::Down => self.scroll.reverse = false,
            Key::Space => self.scroll.fast = !self.scroll.fast,
            _ => return false,
        }
        true
    }

    fn tick(&mut self, env: &mut Env<'_>, out: &mut Vec<ScreenAction>) {
        self.last_tick_ms = Some(env.now_ms);
        self.scroll.tick();
        if self.scroll.finished(self.text_height(), self.gui.1) {
            out.push(ScreenAction::Respawn);
        }
    }

    fn credits_music(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::env::{MemoryClipboard, NoView};
    use crate::gui::screen::ScreenStack;

    fn source() -> CreditsSource {
        CreditsSource {
            poem: Some("Hello PLAYERNAME\n§f§k§a§b\n".into()),
            credits: Some(
                r#"[{"section":"Game","disciplines":[{"discipline":"Code","titles":[{"title":"Dev","names":["Ann","Bob"]}]}]}]"#
                    .into(),
            ),
            postcredits: Some("Bye\n".into()),
        }
    }

    fn texts(lines: &[CreditsLine]) -> Vec<&str> {
        lines.iter().map(|l| l.text.as_str()).collect()
    }

    #[test]
    fn poem_substitutes_the_player_and_masks_the_token() {
        let lines = build_lines(&source(), true, "Steve");
        assert_eq!(lines[0].text, "Hello Steve");
        assert!(lines[0].wrap && lines[1].text.is_empty());
        let masked = &lines[2].text;
        assert!(masked.starts_with("§f§k") && masked.ends_with('X'));
        assert!((3..=6).contains(&(masked.len() - "§f§k".len())), "{masked}");
    }

    #[test]
    fn credits_sections_follow_vanilla_layout() {
        let lines = build_lines(&source(), false, "x");
        assert_eq!(
            texts(&lines),
            [
                "§f============",
                "§eGame",
                "§f============",
                "",
                "",
                "§eCode",
                "",
                "",
                "§7Dev",
                "§f           Ann",
                "§f           Bob",
                "",
                ""
            ]
        );
        assert!(lines[1].centered && !lines[8].centered);
    }

    #[test]
    fn poem_runs_only_for_the_end_and_postcredits_come_last() {
        let with_poem = build_lines(&source(), true, "x");
        assert_eq!(with_poem[0].text, "Hello x");
        assert_eq!(
            texts(&with_poem).iter().rev().find(|t| !t.is_empty()),
            Some(&"Bye")
        );
        let plain = build_lines(&source(), false, "x");
        assert!(!texts(&plain).contains(&"Bye") && !texts(&plain).contains(&"Hello x"));
    }

    #[test]
    fn hostile_files_are_bounded_and_garbage_is_ignored() {
        let huge = CreditsSource {
            poem: Some("a\n".repeat(100_000)),
            credits: Some("not json".into()),
            postcredits: None,
        };
        assert!(build_lines(&huge, true, "x").len() <= MAX_LINES);
    }

    #[test]
    fn scroll_runs_at_vanilla_speeds_and_never_goes_negative() {
        let mut poem = Scroll::new(true);
        poem.tick();
        assert_eq!(poem.offset, 0.5);
        poem.reverse = true;
        for _ in 0..5 {
            poem.tick();
        }
        assert_eq!(poem.offset, 0.0);
        let mut credits = Scroll::new(false);
        credits.fast = true;
        credits.tick();
        assert_eq!(credits.offset, 3.75);
    }

    #[test]
    fn scroll_finishes_after_the_text_and_two_screens() {
        let mut s = Scroll::new(false);
        s.offset = 120.0 + 2.0 * 100.0 + 24.0;
        assert!(!s.finished(120, 100));
        s.tick();
        assert!(s.finished(120, 100));
    }

    fn drive(stack: &mut ScreenStack, input: Option<UiInput>) -> Vec<ScreenAction> {
        let mut clip = MemoryClipboard::default();
        let mut env = Env {
            view: &NoView,
            clipboard: &mut clip,
            now_ms: 0,
        };
        let mut out = Vec::new();
        match input {
            Some(i) => {
                stack.input(&mut env, &i, &mut out);
            }
            None => stack.tick(&mut env, &mut out),
        }
        out
    }

    #[test]
    fn escape_closes_with_a_respawn() {
        let mut stack = ScreenStack::new(320, 240);
        stack.push(Box::new(CreditsScreen::new(&source(), true, "x")));
        assert!(stack.top().is_some_and(Screen::credits_music));
        let out = drive(&mut stack, Some(UiInput::key(Key::Escape)));
        assert!(matches!(out.as_slice(), [ScreenAction::Respawn]));
    }

    #[test]
    fn running_to_the_end_closes_with_a_respawn() {
        let mut stack = ScreenStack::new(320, 240);
        stack.push(Box::new(CreditsScreen::new(&source(), true, "x")));
        let mut ticks = 0;
        loop {
            let out = drive(&mut stack, None);
            ticks += 1;
            if !out.is_empty() {
                assert!(matches!(out.as_slice(), [ScreenAction::Respawn]));
                break;
            }
            assert!(ticks < 100_000, "never finished");
        }
        assert!(ticks > 100);
    }
}
