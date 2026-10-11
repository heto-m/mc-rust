use mcr_ui::{
    Button, Env, Key, LanguageInfo, MouseButton, RenderCx, Screen, ScreenAction, ScrollList,
    UiInput, WidgetEventKind, WidgetSet, draw_menu_background,
};

use crate::settings::SharedSettings;

const DONE: u32 = 1;
const ROW: i32 = 18;
const ROW_WIDTH: i32 = 300;

pub struct LanguageScreen {
    settings: SharedSettings,
    languages: Vec<LanguageInfo>,
    selected: usize,
    list: ScrollList,
    widgets: WidgetSet,
    gui: (i32, i32),
}

impl std::fmt::Debug for LanguageScreen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LanguageScreen")
            .field("selected", &self.selected)
            .finish_non_exhaustive()
    }
}

impl LanguageScreen {
    pub fn new(settings: SharedSettings, languages: Vec<LanguageInfo>) -> Self {
        let current = settings.borrow().file.options.language.clone();
        let selected = languages
            .iter()
            .position(|l| l.code == current)
            .unwrap_or_default();
        Self {
            settings,
            languages,
            selected,
            list: ScrollList::new([0, 32, 0, 0], ROW),
            widgets: WidgetSet::default(),
            gui: (0, 0),
        }
    }

    fn apply(&self) {
        let Some(info) = self.languages.get(self.selected) else {
            return;
        };
        if self.settings.borrow().file.options.language != info.code {
            let code = info.code.clone();
            self.settings
                .borrow_mut()
                .edit(|f| f.options.language = code);
        }
    }

    fn step(&mut self, delta: isize) {
        let last = self.languages.len().saturating_sub(1);
        self.selected = self.selected.saturating_add_signed(delta).min(last);
        self.list.ensure_visible(self.selected);
    }

    fn bar_x(&self) -> i32 {
        self.gui.0 / 2 + ROW_WIDTH / 2 + 10
    }
}

impl Screen for LanguageScreen {
    fn init(&mut self, width: i32, height: i32) {
        self.gui = (width, height);
        self.list
            .set_rect([0, 32, width, (height - 32 - 60).max(0)]);
        self.list.set_count(self.languages.len());
        self.list.ensure_visible(self.selected);
        self.widgets.clear();
        self.widgets.add(Button::new(
            DONE,
            [width / 2 - 100, height - 27, 200, 20],
            "lang:gui.done",
        ));
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        draw_menu_background(cx);
        let title = cx.lang("options.language.title");
        cx.centered_text(&title, self.gui.0 / 2, 12, 0xFFFF_FFFF, true);
        let [lx, ly, lw, lh] = self.list.rect();
        cx.frame.push_scissor(lx, ly, lx + lw, ly + lh);
        let (mx, my) = cx.mouse;
        let hovered = self.list.item_at(mx, my);
        for index in self.list.visible_range() {
            let Some(info) = self.languages.get(index) else {
                continue;
            };
            let y = self.list.item_top(index);
            let left = self.gui.0 / 2 - ROW_WIDTH / 2;
            if index == self.selected {
                cx.frame
                    .fill_rect(left, y - 2, left + ROW_WIDTH, y + ROW, 0xFFFF_FFFF);
                cx.frame.fill_rect(
                    left + 1,
                    y - 1,
                    left + ROW_WIDTH - 1,
                    y + ROW - 1,
                    0xFF00_0000,
                );
            } else if hovered == Some(index) {
                cx.frame
                    .fill_rect(left, y - 2, left + ROW_WIDTH, y + ROW, 0x40FF_FFFF);
            }
            let label = info.label();
            cx.centered_text(&label, self.gui.0 / 2, y + 3, 0xFFFF_FFFF, true);
        }
        cx.frame.pop_scissor();
        self.list.render_scrollbar(cx, self.bar_x());
        let warning = cx.lang("options.languageAccuracyWarning");
        cx.centered_text(&warning, self.gui.0 / 2, self.gui.1 - 50, 0xFFBA_BABA, true);
        self.widgets.render(cx);
    }

    fn input(&mut self, env: &mut Env<'_>, input: &UiInput, out: &mut Vec<ScreenAction>) -> bool {
        match *input {
            UiInput::Key {
                key: Key::Escape, ..
            } => {
                out.push(ScreenAction::Pop);
                return true;
            }
            UiInput::Key {
                key: Key::Enter, ..
            } => {
                self.apply();
                out.push(ScreenAction::Pop);
                return true;
            }
            UiInput::Key { key: Key::Up, .. } => {
                self.step(-1);
                return true;
            }
            UiInput::Key { key: Key::Down, .. } => {
                self.step(1);
                return true;
            }
            UiInput::MousePress {
                x,
                y,
                button: MouseButton::Left,
                ..
            } if mcr_ui::contains(self.list.rect(), x, y)
                && (x - self.gui.0 as f32 / 2.0).abs() <= ROW_WIDTH as f32 / 2.0 =>
            {
                if let Some(i) = self.list.item_at(x, y)
                    && i < self.languages.len()
                {
                    self.selected = i;
                    out.push(ScreenAction::ClickSound);
                    return true;
                }
            }
            _ => {}
        }
        if self.list.input(input, self.bar_x()) {
            return true;
        }
        let mut events = Vec::new();
        let used = self.widgets.input(env, input, &mut events);
        if events
            .iter()
            .any(|e| e.id == DONE && e.kind == WidgetEventKind::Pressed)
        {
            self.apply();
            out.push(ScreenAction::ClickSound);
            out.push(ScreenAction::Pop);
        }
        used
    }
}
