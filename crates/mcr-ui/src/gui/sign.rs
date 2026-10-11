use rustc_hash::FxHashMap;

use super::draw_dim_background;
use super::edit_box::EditBox;
use super::env::{Env, RenderCx};
use super::input::{Key, UiInput};
use super::screen::{Screen, ScreenAction};
use super::widgets::{Button, WidgetEventKind, WidgetSet};
use crate::style::{Cell, FontId, Style};

const WHITE: u32 = 0xFFFF_FFFF;
const SELECTION: u32 = 0xB033_99FF;
const BLINK_MS: u64 = 300;
const DONE: u32 = 0;
const STANDING_SCALE: f32 = 0.976_562_8;
const FALLBACK_ADVANCE: f32 = 6.0;

/// What the sign editor needs to know about the sign, already resolved by the app.
#[derive(Clone, Debug, PartialEq)]
pub struct SignInfo {
    pub wood: String,
    pub hanging: bool,
    pub wall: bool,
    pub lines: [String; 4],
    pub text_color: u32,
}

impl SignInfo {
    fn line_height(&self) -> i32 {
        if self.hanging { 9 } else { 10 }
    }

    fn max_width(&self) -> f32 {
        if self.hanging { 60.0 } else { 90.0 }
    }
}

pub struct SignEditScreen {
    info: SignInfo,
    gui: (i32, i32),
    lines: [EditBox; 4],
    line: usize,
    widgets: WidgetSet,
    advances: FxHashMap<char, f32>,
    unchecked: Option<(usize, EditBox)>,
    focus_ms: u64,
}

impl std::fmt::Debug for SignEditScreen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SignEditScreen")
            .field("line", &self.line)
            .finish()
    }
}

impl SignEditScreen {
    pub fn new(info: SignInfo) -> Self {
        let lines = std::array::from_fn(|i| {
            let mut edit = EditBox::new(i as u32, [0, 0, 0, 0]).with_max_length(384);
            edit.set_value(&info.lines[i]);
            edit.take_changed();
            edit
        });
        Self {
            info,
            gui: (0, 0),
            lines,
            line: 0,
            widgets: WidgetSet::default(),
            advances: FxHashMap::default(),
            unchecked: None,
            focus_ms: 0,
        }
    }

    pub fn text(&self) -> [String; 4] {
        std::array::from_fn(|i| self.lines[i].value().to_owned())
    }

    fn width(&self, text: &str) -> f32 {
        text.chars()
            .map(|c| self.advances.get(&c).copied().unwrap_or(FALLBACK_ADVANCE))
            .sum()
    }

    fn edit(&mut self, apply: impl FnOnce(&mut EditBox) -> bool) -> bool {
        let before = self.lines[self.line].clone();
        let used = apply(&mut self.lines[self.line]);
        let line = &self.lines[self.line];
        if self.width(line.value()) > self.info.max_width() {
            self.lines[self.line] = before;
        } else if line
            .value()
            .chars()
            .any(|c| !self.advances.contains_key(&c))
        {
            self.unchecked = Some((self.line, before));
        }
        used
    }

    fn select_line(&mut self, line: usize, now_ms: u64) {
        self.line = line % 4;
        let end = self.lines[self.line].len();
        self.lines[self.line].move_cursor_to(end, false);
        self.focus_ms = now_ms;
    }

    fn measure(&mut self, cx: &mut RenderCx<'_, '_>) {
        let fonts = cx.frame.ui().fonts_mut();
        for edit in &mut self.lines {
            for c in edit.value().chars() {
                self.advances
                    .entry(c)
                    .or_insert_with(|| fonts.advance(FontId::DEFAULT, c, false));
            }
        }
        for c in ' '..='~' {
            self.advances
                .entry(c)
                .or_insert_with(|| fonts.advance(FontId::DEFAULT, c, false));
        }
        if let Some((line, before)) = self.unchecked.take()
            && self.width(self.lines[line].value()) > self.info.max_width()
        {
            self.lines[line] = before;
        }
    }

    fn done(&self, out: &mut Vec<ScreenAction>) {
        out.push(ScreenAction::SubmitSign(self.text()));
        out.push(ScreenAction::Pop);
    }
}

impl Screen for SignEditScreen {
    fn init(&mut self, width: i32, height: i32) {
        self.gui = (width, height);
        self.widgets.clear();
        self.widgets.add(Button::new(
            DONE,
            [width / 2 - 100, height / 4 + 144, 200, 20],
            "lang:gui.done",
        ));
        self.select_line(self.line, 0);
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        self.measure(cx);
        draw_dim_background(cx);
        let key = if self.info.hanging {
            "hanging_sign.edit"
        } else {
            "sign.edit"
        };
        let title = cx.lang(key);
        cx.centered_text(&title, self.gui.0 / 2, 40, WHITE, true);
        let ox = self.gui.0 as f32 / 2.0;
        self.draw_background(cx, ox);
        self.draw_text(cx, ox);
        self.widgets.render(cx);
    }

    fn input(&mut self, env: &mut Env<'_>, input: &UiInput, out: &mut Vec<ScreenAction>) -> bool {
        match *input {
            UiInput::Key { key, mods, .. } => match key {
                Key::Escape => {
                    self.done(out);
                    return true;
                }
                Key::Up => self.select_line(self.line + 3, env.now_ms),
                Key::Down | Key::Enter => self.select_line(self.line + 1, env.now_ms),
                _ => {
                    let clipboard = &mut *env.clipboard;
                    self.edit(|e| e.key(key, mods, clipboard));
                }
            },
            UiInput::Char(c) => {
                self.edit(|e| e.char(c));
            }
            _ => {
                let mut events = Vec::new();
                self.widgets.input(env, input, &mut events);
                if events
                    .iter()
                    .any(|e| e.id == DONE && e.kind == WidgetEventKind::Pressed)
                {
                    out.push(ScreenAction::ClickSound);
                    self.done(out);
                }
            }
        }
        true
    }
}

impl SignEditScreen {
    fn draw_background(&self, cx: &mut RenderCx<'_, '_>, ox: f32) {
        if self.info.hanging {
            let name = format!("hanging_signs/{}", self.info.wood);
            let (x, y) = (
                (ox - 36.0).round() as i32,
                (125.0f32 - 13.0 - 36.0).round() as i32,
            );
            cx.texture_scaled(&name, (0, 0, 16, 16), (x, y, 72, 72));
        } else {
            let name = format!("signs/{}", self.info.wood);
            let shown = if self.info.wall { 12 } else { 26 };
            let x = (ox - 12.0 * 3.9).round() as i32;
            let y = (90.0f32 + 27.0 - 13.0 * 3.9).round() as i32;
            let (w, h) = (
                (24.0f32 * 3.9).round() as i32,
                (shown as f32 * 3.9).round() as i32,
            );
            cx.texture_scaled(&name, (0, 0, 24, shown), (x, y, w, h));
        }
    }

    fn draw_text(&mut self, cx: &mut RenderCx<'_, '_>, ox: f32) {
        let lh = self.info.line_height();
        let (oy, scale) = if self.info.hanging {
            (125.0f32, 1.0)
        } else {
            (90.0, STANDING_SCALE)
        };
        let mid = 4 * lh / 2;
        let color = self.info.text_color;
        let blink = (cx.now_ms.saturating_sub(self.focus_ms) / BLINK_MS).is_multiple_of(2);
        for i in 0..4 {
            let text = self.lines[i].value().to_owned();
            let width = self.width(&text);
            let x = ox + scale * (-width / 2.0);
            let y = oy + scale * (i as i32 * lh - mid) as f32;
            let cells: Vec<Cell> = text
                .chars()
                .map(|ch| Cell {
                    ch,
                    style: Style::default(),
                })
                .collect();
            cx.frame.cells_scaled(&cells, x, y, color, false, scale);
            if i != self.line {
                continue;
            }
            let (a, b) = self.lines[i].selection();
            let at = |n: usize| {
                let prefix: String = text.chars().take(n).collect();
                ox + scale * (self.width(&prefix) - width / 2.0)
            };
            let cursor_x = at(self.lines[i].cursor());
            if a != b {
                cx.frame.fill_rect(
                    at(a).round() as i32,
                    y.round() as i32,
                    at(b).round() as i32,
                    (y + scale * lh as f32).round() as i32,
                    SELECTION,
                );
            }
            if blink {
                if self.lines[i].cursor() >= self.lines[i].len() {
                    cx.frame.text("_", cursor_x, y, color, false);
                } else {
                    let (cx0, cy) = (cursor_x.round() as i32, y.round() as i32);
                    cx.frame
                        .fill_rect(cx0, cy - 1, cx0 + 1, cy + lh, color | 0xFF00_0000);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::env::{MemoryClipboard, NoView};
    use crate::gui::input::UiInput;

    fn info(hanging: bool) -> SignInfo {
        SignInfo {
            wood: "oak".into(),
            hanging,
            wall: false,
            lines: Default::default(),
            text_color: 0xFF00_0000,
        }
    }

    fn press(screen: &mut SignEditScreen, input: UiInput) -> Vec<ScreenAction> {
        let mut clip = MemoryClipboard::default();
        let mut env = Env {
            view: &NoView,
            clipboard: &mut clip,
            now_ms: 0,
        };
        let mut out = Vec::new();
        screen.input(&mut env, &input, &mut out);
        out
    }

    #[test]
    fn typing_goes_to_the_selected_line_and_enter_advances() {
        let mut s = SignEditScreen::new(info(false));
        s.init(320, 240);
        press(&mut s, UiInput::Char('a'));
        press(&mut s, UiInput::key(Key::Enter));
        press(&mut s, UiInput::Char('b'));
        press(&mut s, UiInput::key(Key::Up));
        press(&mut s, UiInput::Char('c'));
        assert_eq!(
            s.text(),
            ["ac".to_owned(), "b".into(), "".into(), "".into()]
        );
    }

    #[test]
    fn lines_wrap_around() {
        let mut s = SignEditScreen::new(info(false));
        s.init(320, 240);
        press(&mut s, UiInput::key(Key::Up));
        press(&mut s, UiInput::Char('z'));
        assert_eq!(s.text()[3], "z");
        press(&mut s, UiInput::key(Key::Down));
        press(&mut s, UiInput::Char('y'));
        assert_eq!(s.text()[0], "y");
    }

    #[test]
    fn width_limit_rejects_overflow_with_the_fallback_advance() {
        let mut s = SignEditScreen::new(info(true));
        s.init(320, 240);
        for _ in 0..20 {
            press(&mut s, UiInput::Char('w'));
        }
        assert_eq!(s.text()[0].chars().count(), 10);
    }

    #[test]
    fn escape_and_done_submit_then_close() {
        let mut s = SignEditScreen::new(info(false));
        s.init(320, 240);
        press(&mut s, UiInput::Char('q'));
        let out = press(&mut s, UiInput::key(Key::Escape));
        assert!(matches!(
            out.as_slice(),
            [ScreenAction::SubmitSign(t), ScreenAction::Pop] if t[0] == "q"
        ));
    }
}
