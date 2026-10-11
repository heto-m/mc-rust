use mcr_protocol::types::text::ClickEvent;

use super::menus::ConfirmScreen;
use super::screen::ScreenAction;

/// Turns a click event from server text into the screen it needs: links, commands and copies always
/// go through a confirmation that shows exactly what will happen, everything else does nothing.
pub fn click_event_actions(event: ClickEvent, out: &mut Vec<ScreenAction>) {
    let confirm = match event {
        ClickEvent::OpenUrl(url) => ConfirmScreen::link(url),
        ClickEvent::RunCommand(command) => ConfirmScreen::command(&command),
        ClickEvent::CopyToClipboard(text) => ConfirmScreen::copy(&text),
        ClickEvent::ShowDialog(_)
        | ClickEvent::Custom { .. }
        | ClickEvent::OpenFile(_)
        | ClickEvent::SuggestCommand(_)
        | ClickEvent::ChangePage(_) => None,
    };
    if let Some(confirm) = confirm {
        out.push(ScreenAction::Push(Box::new(confirm)));
    }
}
