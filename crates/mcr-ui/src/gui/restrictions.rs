//! Vanilla's `RestrictionsScreen`: why chat is limited and what the player may do in chat.

use super::env::{Env, RenderCx};
use super::input::{Key, UiInput};
use super::menus::{WHITE, draw_dim_background};
use super::screen::{Screen, ScreenAction};
use super::widgets::{Button, WidgetEvent, WidgetEventKind, WidgetSet};

const DONE: u32 = 1;
const ACTION: u32 = 2;
const TEXT_WIDTH: i32 = 250;
const RED: &str = "§c";
const GREEN: &str = "§a";

/// The chat limits vanilla's `ChatAbilities` derives from the chat visibility option.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChatLimit {
    #[default]
    None,
    /// Chat visibility "Commands Only".
    PlayerChat,
    /// Chat visibility "Hidden".
    ChatAndCommands,
}

impl ChatLimit {
    fn reason(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::PlayerChat => Some("chat_restriction.chat_disabled_by_options"),
            Self::ChatAndCommands => Some("chat_restriction.chat_and_commands_disabled_by_options"),
        }
    }

    pub fn send_messages(self) -> bool {
        self == Self::None
    }

    pub fn send_commands(self) -> bool {
        self != Self::ChatAndCommands
    }

    pub fn receive_player_messages(self) -> bool {
        self == Self::None
    }

    pub fn receive_system_messages(self) -> bool {
        self != Self::ChatAndCommands
    }
}

#[derive(Debug)]
pub struct RestrictionsScreen {
    widgets: WidgetSet,
    gui: (i32, i32),
    limit: ChatLimit,
}

impl RestrictionsScreen {
    pub fn new(limit: ChatLimit) -> Self {
        Self {
            widgets: WidgetSet::default(),
            gui: (0, 0),
            limit,
        }
    }

    fn permissions(&self) -> [(&'static str, bool); 4] {
        let l = self.limit;
        [
            ("send_messages", l.send_messages()),
            ("send_commands", l.send_commands()),
            ("receive_system_messages", l.receive_system_messages()),
            ("receive_player_messages", l.receive_player_messages()),
        ]
    }
}

fn wrap(cx: &mut RenderCx<'_, '_>, text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let next = if line.is_empty() {
            word.to_owned()
        } else {
            format!("{line} {word}")
        };
        if !line.is_empty() && cx.text_width(&next) > TEXT_WIDTH {
            lines.push(std::mem::replace(&mut line, word.to_owned()));
        } else {
            line = next;
        }
    }
    lines.push(line);
    lines
}

impl Screen for RestrictionsScreen {
    fn init(&mut self, width: i32, height: i32) {
        self.gui = (width, height);
        self.widgets.clear();
        if self.limit.reason().is_some() {
            self.widgets.add(Button::new(
                ACTION,
                [width / 2 - 100, 56, 200, 20],
                "lang:chat_restriction.chat_disabled_by_options.action",
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
        let center = self.gui.0 / 2;
        let title = cx.lang("restrictions_screen.title");
        cx.centered_text(&title, center, 12, WHITE, true);
        let mut y = 36;
        if let Some(key) = self.limit.reason() {
            let reason = cx.lang(key);
            for line in wrap(cx, &reason) {
                cx.centered_text(&format!("{RED}§l{line}"), center, y, WHITE, true);
                y += 10;
            }
            y = 56 + 20 + 14;
        }
        for (id, allowed) in self.permissions() {
            let suffix = if allowed { "allowed" } else { "denied" };
            let text = cx.lang(&format!("restrictions_screen.permission.{id}.{suffix}"));
            let color = if allowed { GREEN } else { RED };
            cx.centered_text(&format!("{color}{text}"), center, y, WHITE, true);
            y += 12;
        }
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
            match (id, kind) {
                (DONE, WidgetEventKind::Pressed) => {
                    out.push(ScreenAction::ClickSound);
                    out.push(ScreenAction::Pop);
                }
                (ACTION, WidgetEventKind::Pressed) => {
                    out.push(ScreenAction::ClickSound);
                    out.push(ScreenAction::OpenOptions);
                }
                _ => {}
            }
        }
        used
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_follow_the_chat_visibility_option() {
        let rows = |l| RestrictionsScreen::new(l).permissions().map(|(_, ok)| ok);
        assert_eq!(rows(ChatLimit::None), [true; 4]);
        assert_eq!(rows(ChatLimit::PlayerChat), [false, true, true, false]);
        assert_eq!(
            rows(ChatLimit::ChatAndCommands),
            [false, false, false, false]
        );
    }

    #[test]
    fn only_a_limited_chat_has_a_reason() {
        assert_eq!(ChatLimit::None.reason(), None);
        assert!(ChatLimit::PlayerChat.reason().is_some());
    }
}
