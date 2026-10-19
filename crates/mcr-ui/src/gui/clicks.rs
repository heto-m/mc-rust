use mcr_protocol::types::text::ClickEvent;

use super::menus::{is_plain, parse_web_url};
use super::screen::ScreenAction;

/// Longest clicked command or copied text, the protocol's string limit.
pub const MAX_CLICK_TEXT: usize = 32_767;

/// A clicked command without its optional `/`, if it is printable text the server accepts.
pub fn clicked_command(command: &str) -> Option<String> {
    let command = command.strip_prefix('/').unwrap_or(command);
    (!command.is_empty() && command.chars().count() <= MAX_CLICK_TEXT && is_plain(command))
        .then(|| command.to_owned())
}

/// Vanilla `Screen#defaultHandleGameClickEvent` for server text: commands go through the app's command
/// check, URLs through the link options and confirmation, dialogs and custom actions to the server,
/// copies happen at once. Values that could hide what they do are dropped.
pub fn click_event_actions(event: ClickEvent, out: &mut Vec<ScreenAction>) {
    match event {
        ClickEvent::OpenUrl(url) => {
            if parse_web_url(&url).is_some() {
                out.push(ScreenAction::OpenLink(url));
            }
        }
        ClickEvent::RunCommand(command) => {
            if let Some(command) = clicked_command(&command) {
                out.push(ScreenAction::RunCommand(command));
            }
        }
        ClickEvent::CopyToClipboard(text) => {
            if !text.is_empty() && text.chars().count() <= MAX_CLICK_TEXT && is_plain(&text) {
                out.push(ScreenAction::CopyToClipboard(text));
            }
        }
        ClickEvent::ShowDialog(dialog) => out.push(ScreenAction::ShowDialog(dialog)),
        ClickEvent::Custom { id, payload } => out.push(ScreenAction::CustomClick { id, payload }),
        ClickEvent::OpenFile(_) | ClickEvent::SuggestCommand(_) | ClickEvent::ChangePage(_) => {}
    }
}
