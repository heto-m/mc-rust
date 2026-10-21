//! `DebugOptionsScreen` (F3+F6): switches for the debug renderers and charts this client has.

use super::env::{Env, RenderCx};
use super::input::{Key, UiInput};
use super::menus::{WHITE, draw_dim_background};
use super::screen::{DebugToggle, Screen, ScreenAction};
use super::widgets::{Button, CycleButton, WidgetEvent, WidgetEventKind, WidgetSet};

const DONE: u32 = 100;

const ROWS: [(DebugToggle, &str); 7] = [
    (DebugToggle::Hitboxes, "Entity Hitboxes"),
    (DebugToggle::ChunkBorders, "Chunk Borders"),
    (DebugToggle::AdvancedTooltips, "Advanced Tooltips"),
    (DebugToggle::FpsCharts, "FPS Charts"),
    (DebugToggle::NetworkCharts, "Network Charts"),
    (DebugToggle::LightmapTexture, "Lightmap Texture"),
    (DebugToggle::ProfilerChart, "Profiler Chart"),
];

#[derive(Debug)]
pub struct DebugOptionsScreen {
    widgets: WidgetSet,
    gui: (i32, i32),
    on: [bool; ROWS.len()],
}

impl DebugOptionsScreen {
    /// `on` follows the order of [`DebugToggle::ALL`].
    pub fn new(on: [bool; ROWS.len()]) -> Self {
        Self {
            widgets: WidgetSet::default(),
            gui: (0, 0),
            on,
        }
    }

    pub fn state(&self, toggle: DebugToggle) -> bool {
        ROWS.iter()
            .position(|(t, _)| *t == toggle)
            .is_some_and(|i| self.on[i])
    }
}

impl Screen for DebugOptionsScreen {
    fn init(&mut self, width: i32, height: i32) {
        self.gui = (width, height);
        self.widgets.clear();
        let columns = if width >= 330 { 2 } else { 1 };
        let left = width / 2 - if columns == 2 { 155 } else { 75 };
        for (i, (_, label)) in ROWS.iter().enumerate() {
            let (col, row) = (i % columns, i / columns);
            self.widgets.add(CycleButton::new(
                i as u32,
                [left + col as i32 * 160, 62 + row as i32 * 24, 150, 20],
                *label,
                vec!["lang:options.on".into(), "lang:options.off".into()],
                usize::from(!self.on[i]),
            ));
        }
        self.widgets.add(Button::new(
            DONE,
            [width / 2 - 100, height - 28, 200, 20],
            "lang:gui.done",
        ));
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        draw_dim_background(cx);
        let title = cx.lang("debug.options.title");
        cx.centered_text(&title, self.gui.0 / 2, 12, WHITE, true);
        let warning = cx.lang("debug.options.warning");
        let limit = self.gui.0 - 20;
        let mut line = String::new();
        let mut y = 24;
        for word in warning.split_whitespace() {
            let next = if line.is_empty() {
                word.to_owned()
            } else {
                format!("{line} {word}")
            };
            if !line.is_empty() && cx.text_width(&next) > limit {
                cx.centered_text(&format!("§7{line}"), self.gui.0 / 2, y, WHITE, true);
                y += 10;
                line = word.to_owned();
            } else {
                line = next;
            }
        }
        cx.centered_text(&format!("§7{line}"), self.gui.0 / 2, y, WHITE, true);
        self.widgets.render(cx);
    }

    fn input(&mut self, env: &mut Env<'_>, input: &UiInput, out: &mut Vec<ScreenAction>) -> bool {
        if let UiInput::Key {
            key: Key::Escape, ..
        } = input
        {
            out.push(ScreenAction::Pop);
            return true;
        }
        let mut events = Vec::new();
        let used = self.widgets.input(env, input, &mut events);
        for WidgetEvent { id, kind } in events {
            match kind {
                WidgetEventKind::Cycled(i) if (id as usize) < ROWS.len() => {
                    self.on[id as usize] = i == 0;
                    out.push(ScreenAction::ClickSound);
                    out.push(ScreenAction::ToggleDebug(ROWS[id as usize].0));
                }
                WidgetEventKind::Pressed if id == DONE => {
                    out.push(ScreenAction::ClickSound);
                    out.push(ScreenAction::Pop);
                }
                _ => {}
            }
        }
        used
    }

    fn is_debug_options(&self) -> bool {
        true
    }
}

impl DebugToggle {
    pub const ALL: [Self; ROWS.len()] = [
        Self::Hitboxes,
        Self::ChunkBorders,
        Self::AdvancedTooltips,
        Self::FpsCharts,
        Self::NetworkCharts,
        Self::LightmapTexture,
        Self::ProfilerChart,
    ];
}
