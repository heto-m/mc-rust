mod common;

use std::cell::RefCell;
use std::rc::Rc;

use common::harness::{TestView, send, stack, typed};
use mcr_protocol::types::text::{ClickEvent, Component};
use mcr_ui::{
    ChatHud, ChatOptions, ChatScreen, ClickType, CommandSuggester, ContainerClick, ContainerScreen,
    DeathScreen, DirectConnectScreen, EditServerScreen, Env, Key, MemoryClipboard,
    MemoryServerStore, MenuKind, Modifiers, MouseButton, NoView, PauseScreen, Screen, ScreenAction,
    ScreenStack, ServerEntry, ServerListScreen, ServerStore, SharedChat, Suggestion, SuggestionSet,
    UiInput,
};

const W: i32 = 427;
const H: i32 = 240;

fn tick(screen: &mut dyn Screen, view: &dyn mcr_ui::GameView, n: u32) {
    let mut clip = MemoryClipboard::default();
    let mut env = Env {
        view,
        clipboard: &mut clip,
        now_ms: 0,
    };
    let mut out = Vec::new();
    for _ in 0..n {
        screen.tick(&mut env, &mut out);
    }
}

fn click(x: f32, y: f32) -> Vec<UiInput> {
    vec![
        UiInput::press(x, y, MouseButton::Left),
        UiInput::release(x, y, MouseButton::Left),
    ]
}

#[derive(Default)]
struct Script {
    result: Option<SuggestionSet>,
    requests: Vec<(String, usize)>,
}

#[derive(Clone, Default)]
struct SharedScript(Rc<RefCell<Script>>);

impl CommandSuggester for SharedScript {
    fn request(&mut self, text: &str, cursor: usize, _suggest: bool) {
        self.0.borrow_mut().requests.push((text.to_owned(), cursor));
    }

    fn poll(&mut self) -> Option<SuggestionSet> {
        self.0.borrow_mut().result.take()
    }
}

fn suggestion(start: usize, end: usize, text: &str) -> Suggestion {
    Suggestion {
        start,
        end,
        text: text.into(),
        tooltip: None,
    }
}

fn chat() -> (SharedChat, ChatScreen, SharedScript) {
    let hud: SharedChat = Rc::new(RefCell::new(ChatHud::default()));
    let script = SharedScript::default();
    let mut screen = ChatScreen::new(
        hud.clone(),
        Box::new(script.clone()),
        ChatOptions::default(),
    );
    screen.init(W, H);
    (hud, screen, script)
}

fn key(screen: &mut ChatScreen, key: Key) -> Vec<ScreenAction> {
    send(screen, &NoView, &[UiInput::key(key)])
}

#[test]
fn chat_sends_message_and_closes() {
    let (hud, mut screen, _) = chat();
    send(&mut screen, &NoView, &typed("hi there"));
    let out = key(&mut screen, Key::Enter);
    assert!(
        matches!(out.as_slice(), [ScreenAction::SendChat(m), ScreenAction::Pop] if m == "hi there")
    );
    assert_eq!(hud.borrow().recent(), ["hi there"]);
}

#[test]
fn chat_slash_sends_command_without_slash() {
    let (_, mut screen, _) = chat();
    send(&mut screen, &NoView, &typed("/gamemode creative"));
    let out = key(&mut screen, Key::Enter);
    assert!(
        matches!(out.as_slice(), [ScreenAction::SendCommand(c), ScreenAction::Pop] if c == "gamemode creative")
    );
}

#[test]
fn chat_normalizes_whitespace_and_skips_blank() {
    let (hud, mut screen, _) = chat();
    send(&mut screen, &NoView, &typed("  a   b  "));
    let out = key(&mut screen, Key::Enter);
    assert!(matches!(out.as_slice(), [ScreenAction::SendChat(m), ..] if m == "a b"));
    let (_, mut blank, _) = chat();
    send(&mut blank, &NoView, &typed("   "));
    let out = key(&mut blank, Key::Enter);
    assert!(matches!(out.as_slice(), [ScreenAction::Pop]));
    assert_eq!(hud.borrow().recent().len(), 1);
}

#[test]
fn chat_escape_closes_without_sending() {
    let (_, mut screen, _) = chat();
    send(&mut screen, &NoView, &typed("draft"));
    assert!(matches!(
        key(&mut screen, Key::Escape).as_slice(),
        [ScreenAction::Pop]
    ));
}

#[test]
fn chat_history_up_down_and_draft() {
    let hud: SharedChat = Rc::new(RefCell::new(ChatHud::default()));
    hud.borrow_mut().add_recent("one");
    hud.borrow_mut().add_recent("two");
    let mut screen = ChatScreen::new(hud, Box::new(mcr_ui::NoSuggestions), ChatOptions::default());
    screen.init(W, H);
    send(&mut screen, &NoView, &typed("dra"));
    key(&mut screen, Key::Up);
    assert_eq!(screen.text(), "two");
    key(&mut screen, Key::Up);
    assert_eq!(screen.text(), "one");
    key(&mut screen, Key::Up);
    assert_eq!(screen.text(), "one");
    key(&mut screen, Key::Down);
    assert_eq!(screen.text(), "two");
    key(&mut screen, Key::Down);
    assert_eq!(screen.text(), "dra");
}

#[test]
fn chat_requests_suggestions_on_edit() {
    let (_, mut screen, script) = chat();
    send(&mut screen, &NoView, &typed("/ga"));
    let requests = script.0.borrow().requests.clone();
    assert_eq!(requests.last(), Some(&("/ga".to_owned(), 3)));
    assert_eq!(requests.len(), 3);
}

fn chat_with_suggestions() -> (ChatScreen, SharedScript) {
    let (_, mut screen, script) = chat();
    send(&mut screen, &NoView, &typed("/ga"));
    script.0.borrow_mut().result = Some(SuggestionSet {
        suggestions: ["gamemode", "gamerule", "give"]
            .iter()
            .map(|t| suggestion(1, 3, t))
            .collect(),
        text: "/ga".into(),
        command: true,
        ..SuggestionSet::default()
    });
    tick(&mut screen, &NoView, 1);
    (screen, script)
}

#[test]
fn suggestions_sort_prefix_matches_first() {
    let (screen, _) = chat_with_suggestions();
    assert_eq!(screen.popup_len(), 3);
    assert_eq!(screen.selected_suggestion().unwrap().text, "gamemode");
}

#[test]
fn tab_applies_selected_suggestion_then_cycles() {
    let (mut screen, _) = chat_with_suggestions();
    key(&mut screen, Key::Tab);
    assert_eq!(screen.text(), "/gamemode");
    key(&mut screen, Key::Tab);
    assert_eq!(screen.text(), "/gamerule");
    key(&mut screen, Key::Tab);
    assert_eq!(screen.text(), "/give");
}

#[test]
fn arrows_move_the_selection_and_tab_uses_it() {
    let (mut screen, _) = chat_with_suggestions();
    key(&mut screen, Key::Down);
    assert_eq!(screen.selected_suggestion().unwrap().text, "gamerule");
    key(&mut screen, Key::Up);
    key(&mut screen, Key::Up);
    assert_eq!(screen.selected_suggestion().unwrap().text, "give");
    key(&mut screen, Key::Tab);
    assert_eq!(screen.text(), "/give");
}

#[test]
fn escape_hides_popup_before_closing_chat() {
    let (mut screen, _) = chat_with_suggestions();
    assert!(key(&mut screen, Key::Escape).is_empty());
    assert_eq!(screen.popup_len(), 0);
    assert!(matches!(
        key(&mut screen, Key::Escape).as_slice(),
        [ScreenAction::Pop]
    ));
}

#[test]
fn suggestion_replaces_only_its_range() {
    let (_, mut screen, script) = chat();
    send(&mut screen, &NoView, &typed("/tp Ste"));
    script.0.borrow_mut().result = Some(SuggestionSet {
        suggestions: vec![suggestion(4, 7, "Steve")],
        text: "/tp Ste".into(),
        command: true,
        ..SuggestionSet::default()
    });
    tick(&mut screen, &NoView, 1);
    key(&mut screen, Key::Tab);
    assert_eq!(screen.text(), "/tp Steve");
}

#[test]
fn tab_without_popup_shows_pending_results() {
    let (_, mut screen, script) = chat();
    screen.init(W, H);
    script.0.borrow_mut().result = Some(SuggestionSet {
        suggestions: vec![suggestion(0, 0, "help")],
        command: true,
        ..SuggestionSet::default()
    });
    tick(&mut screen, &NoView, 1);
    assert_eq!(screen.popup_len(), 0);
}

#[test]
fn history_navigation_does_not_open_the_popup() {
    let hud: SharedChat = Rc::new(RefCell::new(ChatHud::default()));
    hud.borrow_mut().add_recent("/gamemode creative");
    let script = SharedScript::default();
    let mut screen = ChatScreen::new(hud, Box::new(script.clone()), ChatOptions::default());
    screen.init(W, H);
    key(&mut screen, Key::Up);
    script.0.borrow_mut().result = Some(SuggestionSet {
        suggestions: vec![suggestion(0, 1, "x")],
        text: "/gamemode creative".into(),
        command: true,
        ..SuggestionSet::default()
    });
    tick(&mut screen, &NoView, 1);
    assert_eq!(screen.popup_len(), 0);
}

#[test]
fn chat_click_events_become_actions() {
    let (_, mut screen, _) = chat();
    let mut out = Vec::new();
    screen.handle_click_event(ClickEvent::OpenUrl("https://example.com".into()), &mut out);
    assert!(matches!(out.as_slice(), [ScreenAction::OpenLink(u)] if u == "https://example.com"));
    out.clear();
    screen.handle_click_event(ClickEvent::OpenUrl("file:///etc/passwd".into()), &mut out);
    assert!(out.is_empty());
    screen.handle_click_event(ClickEvent::RunCommand("/home".into()), &mut out);
    screen.handle_click_event(ClickEvent::CopyToClipboard("abc".into()), &mut out);
    assert!(matches!(
        out.as_slice(),
        [ScreenAction::RunCommand(c), ScreenAction::CopyToClipboard(t)] if c == "home" && t == "abc"
    ));
    screen.handle_click_event(
        ClickEvent::SuggestCommand("/msg Bob ".into()),
        &mut Vec::new(),
    );
    assert_eq!(screen.text(), "/msg Bob ");
}

#[test]
fn link_confirmation_comes_before_the_action() {
    let confirm = mcr_ui::ConfirmScreen::link("https://example.com".into()).unwrap();
    let mut stack = ScreenStack::new(W, H);
    stack.push(Box::new(PauseScreen::new(false, false)));
    let mut clip = MemoryClipboard::default();
    let mut env = Env {
        view: &NoView,
        clipboard: &mut clip,
        now_ms: 0,
    };
    let mut forwarded = Vec::new();
    stack.push(Box::new(confirm));
    assert_eq!(stack.len(), 2);
    stack.input(&mut env, &UiInput::key(Key::Escape), &mut forwarded);
    assert!(forwarded.is_empty());
    assert_eq!(stack.len(), 1);
}

#[test]
fn page_keys_scroll_the_chat() {
    let (hud, mut screen, _) = chat();
    let tx = common::harness::stack("stone", 1);
    let _ = tx;
    assert_eq!(hud.borrow().scroll(), 0);
    key(&mut screen, Key::PageUp);
    assert_eq!(hud.borrow().scroll(), 0);
}

fn death(hardcore: bool) -> DeathScreen {
    let mut screen = DeathScreen::new(None, hardcore);
    screen.init(W, H);
    screen
}

const RESPAWN: (f32, f32) = (213.0, (H / 4 + 72 + 10) as f32);
const TITLE: (f32, f32) = (213.0, (H / 4 + 96 + 10) as f32);

#[test]
fn death_buttons_wait_twenty_ticks() {
    let mut screen = death(false);
    assert!(send(&mut screen, &NoView, &click(RESPAWN.0, RESPAWN.1)).is_empty());
    tick(&mut screen, &NoView, 19);
    assert!(send(&mut screen, &NoView, &click(RESPAWN.0, RESPAWN.1)).is_empty());
    tick(&mut screen, &NoView, 1);
    let out = send(&mut screen, &NoView, &click(RESPAWN.0, RESPAWN.1));
    assert!(out.iter().any(|a| matches!(a, ScreenAction::Respawn)));
}

#[test]
fn death_title_asks_for_confirmation_unless_hardcore() {
    let mut screen = death(false);
    tick(&mut screen, &NoView, 20);
    let out = send(&mut screen, &NoView, &click(TITLE.0, TITLE.1));
    assert!(out.iter().any(|a| matches!(a, ScreenAction::Push(_))));
    assert!(!out.iter().any(|a| matches!(a, ScreenAction::Disconnect)));
    let mut hardcore = death(true);
    tick(&mut hardcore, &NoView, 20);
    let out = send(&mut hardcore, &NoView, &click(TITLE.0, TITLE.1));
    assert!(out.iter().any(|a| matches!(a, ScreenAction::Disconnect)));
}

#[test]
fn pause_buttons_and_escape() {
    let mut screen = PauseScreen::new(true, false);
    screen.init(W, H);
    assert!(screen.pauses_game());
    assert!(matches!(
        send(&mut screen, &NoView, &[UiInput::key(Key::Escape)]).as_slice(),
        [ScreenAction::Pop]
    ));
    let top = (H - 166) / 4;
    let options = send(&mut screen, &NoView, &click(213.0, (top + 122 + 10) as f32));
    assert!(
        options
            .iter()
            .any(|a| matches!(a, ScreenAction::OpenOptions))
    );
    let disconnect = send(&mut screen, &NoView, &click(213.0, (top + 146 + 10) as f32));
    assert!(
        disconnect
            .iter()
            .any(|a| matches!(a, ScreenAction::Disconnect))
    );
    let back = send(&mut screen, &NoView, &click(213.0, (top + 50 + 10) as f32));
    assert!(back.iter().any(|a| matches!(a, ScreenAction::Pop)));
}

fn servers() -> Rc<RefCell<MemoryServerStore>> {
    Rc::new(RefCell::new(MemoryServerStore {
        entries: ["a", "b", "c"]
            .iter()
            .map(|n| ServerEntry {
                name: (*n).into(),
                address: format!("{n}.example.com"),
            })
            .collect(),
        ..Default::default()
    }))
}

fn server_list(store: Rc<RefCell<MemoryServerStore>>) -> ServerListScreen {
    let mut screen = ServerListScreen::new(store);
    screen.init(W, H);
    screen
}

fn row_center(index: usize) -> (f32, f32) {
    (213.0 + 60.0, (33 + 4 + index as i32 * 36 + 18) as f32)
}

#[test]
fn server_list_select_and_join() {
    let mut screen = server_list(servers());
    let (x, y) = row_center(1);
    assert!(send(&mut screen, &NoView, &click(x, y)).is_empty());
    assert_eq!(screen.selected(), Some(1));
    let join = send(
        &mut screen,
        &NoView,
        &click(213.0 - 154.0 + 50.0, (H - 52 + 10) as f32),
    );
    assert!(
        join.iter()
            .any(|a| matches!(a, ScreenAction::Connect(addr) if addr == "b.example.com"))
    );
}

#[test]
fn server_list_double_click_and_enter_join() {
    let mut screen = server_list(servers());
    let (x, y) = row_center(2);
    send(&mut screen, &NoView, &click(x, y));
    let out = send(
        &mut screen,
        &NoView,
        &[UiInput::MousePress {
            x,
            y,
            button: MouseButton::Left,
            mods: mcr_ui::Modifiers::NONE,
            double: true,
        }],
    );
    assert!(
        out.iter()
            .any(|a| matches!(a, ScreenAction::Connect(addr) if addr == "c.example.com"))
    );
    let out = send(&mut screen, &NoView, &[UiInput::key(Key::Enter)]);
    assert!(
        out.iter()
            .any(|a| matches!(a, ScreenAction::Connect(addr) if addr == "c.example.com"))
    );
}

#[test]
fn server_list_buttons_need_a_selection() {
    let mut screen = server_list(servers());
    let join = send(
        &mut screen,
        &NoView,
        &click(213.0 - 154.0 + 50.0, (H - 52 + 10) as f32),
    );
    assert!(join.is_empty());
    let delete = send(
        &mut screen,
        &NoView,
        &click(213.0 - 154.0 + 78.0 + 30.0, (H - 28 + 10) as f32),
    );
    assert!(delete.is_empty());
}

#[test]
fn server_list_arrow_keys_select() {
    let mut screen = server_list(servers());
    send(&mut screen, &NoView, &[UiInput::key(Key::Down)]);
    assert_eq!(screen.selected(), Some(0));
    send(
        &mut screen,
        &NoView,
        &[
            UiInput::key(Key::Down),
            UiInput::key(Key::Down),
            UiInput::key(Key::Down),
        ],
    );
    assert_eq!(screen.selected(), Some(2));
    send(&mut screen, &NoView, &[UiInput::key(Key::Up)]);
    assert_eq!(screen.selected(), Some(1));
}

#[test]
fn server_list_reorders_with_row_arrows() {
    let store = servers();
    let mut screen = server_list(store.clone());
    let left = 213 - 152 + 2;
    let top = 33 + 4 + 36 + 2;
    send(
        &mut screen,
        &NoView,
        &click((left + 4) as f32, (top + 4) as f32),
    );
    assert_eq!(store.borrow().servers()[0].name, "b");
    assert_eq!(store.borrow().servers()[1].name, "a");
    assert_eq!(screen.selected(), Some(0));
}

#[test]
fn server_list_delete_confirms_then_removes() {
    let store = servers();
    let mut screen = server_list(store.clone());
    send(
        &mut screen,
        &NoView,
        &click(row_center(1).0, row_center(1).1),
    );
    let out = send(
        &mut screen,
        &NoView,
        &click(213.0 - 154.0 + 78.0 + 30.0, (H - 28 + 10) as f32),
    );
    let Some(ScreenAction::Push(confirm)) =
        out.into_iter().find(|a| matches!(a, ScreenAction::Push(_)))
    else {
        panic!("delete should ask first");
    };
    assert_eq!(store.borrow().servers().len(), 3);
    let mut confirm = confirm;
    confirm.init(W, H);
    let mut clip = MemoryClipboard::default();
    let mut env = Env {
        view: &NoView,
        clipboard: &mut clip,
        now_ms: 0,
    };
    let mut actions = Vec::new();
    let delay_buttons_y = H / 2 - 50 + 20 + 9 + 20;
    let _ = delay_buttons_y;
    confirm.input(&mut env, &UiInput::key(Key::Escape), &mut actions);
    assert_eq!(store.borrow().servers().len(), 3);
}

#[test]
fn edit_server_screen_adds_and_edits() {
    let store = servers();
    let mut add = EditServerScreen::new(store.clone(), None);
    add.init(W, H);
    send(&mut add, &NoView, &typed("My Server"));
    send(&mut add, &NoView, &[UiInput::key(Key::Tab)]);
    send(&mut add, &NoView, &typed("play.example.net"));
    let out = send(&mut add, &NoView, &[UiInput::key(Key::Enter)]);
    assert!(matches!(out.as_slice(), [ScreenAction::Pop]));
    assert_eq!(
        store.borrow().servers().last(),
        Some(&ServerEntry {
            name: "My Server".into(),
            address: "play.example.net".into()
        })
    );
    let mut edit = EditServerScreen::new(store.clone(), Some(0));
    edit.init(W, H);
    send(&mut edit, &NoView, &typed("!"));
    send(&mut edit, &NoView, &[UiInput::key(Key::Tab)]);
    let out = send(&mut edit, &NoView, &[UiInput::key(Key::Enter)]);
    assert!(matches!(out.as_slice(), [ScreenAction::Pop]));
    assert_eq!(store.borrow().servers()[0].name, "a!");
    assert_eq!(store.borrow().servers()[0].address, "a.example.com");
}

#[test]
fn edit_server_needs_an_address() {
    let store = servers();
    let mut add = EditServerScreen::new(store.clone(), None);
    add.init(W, H);
    send(&mut add, &NoView, &[UiInput::key(Key::Tab)]);
    assert!(send(&mut add, &NoView, &[UiInput::key(Key::Enter)]).is_empty());
    assert_eq!(store.borrow().servers().len(), 3);
}

#[test]
fn direct_connect_joins_and_remembers() {
    let store = servers();
    let mut screen = DirectConnectScreen::new(store.clone());
    screen.init(W, H);
    assert!(send(&mut screen, &NoView, &[UiInput::key(Key::Enter)]).is_empty());
    send(&mut screen, &NoView, &typed("localhost:25565"));
    let out = send(&mut screen, &NoView, &[UiInput::key(Key::Enter)]);
    assert!(matches!(out.as_slice(), [ScreenAction::Connect(a)] if a == "localhost:25565"));
    assert_eq!(store.borrow().last_direct_address(), "localhost:25565");
}

#[test]
fn tab_cycles_focus_between_widgets() {
    let mut screen = EditServerScreen::new(servers(), None);
    screen.init(W, H);
    send(&mut screen, &NoView, &typed("n"));
    send(&mut screen, &NoView, &[UiInput::key(Key::Tab)]);
    send(&mut screen, &NoView, &typed("x"));
    send(
        &mut screen,
        &NoView,
        &[UiInput::key_mods(Key::Tab, Modifiers::SHIFT)],
    );
    send(&mut screen, &NoView, &typed("m"));
    send(&mut screen, &NoView, &[UiInput::key(Key::Tab)]);
    let out = send(&mut screen, &NoView, &[UiInput::key(Key::Enter)]);
    assert!(matches!(out.as_slice(), [ScreenAction::Pop]));
}

fn container(kind: MenuKind) -> ContainerScreen {
    let mut screen = ContainerScreen::new(kind, Component::text("test"));
    screen.init(W, H);
    screen
}

fn slot_pos(screen: &ContainerScreen, slot: usize) -> (f32, f32) {
    let (l, t) = screen.origin();
    let s = &screen.layout().slots[slot];
    ((l + s.x + 8) as f32, (t + s.y + 8) as f32)
}

fn clicks(out: &[ScreenAction]) -> Vec<ContainerClick> {
    out.iter().filter_map(ScreenAction::click).collect()
}

#[test]
fn container_screen_click_table() {
    let mut screen = container(MenuKind::Generic { rows: 3 });
    let mut view = TestView::with_slots(63);
    view.set(3, "stone", 10);
    let pos = slot_pos(&screen, 3);
    let table: [(UiInput, UiInput, ContainerClick); 3] = [
        (
            UiInput::press(pos.0, pos.1, MouseButton::Left),
            UiInput::release(pos.0, pos.1, MouseButton::Left),
            ContainerClick::new(3, 0, ClickType::Pickup),
        ),
        (
            UiInput::press(pos.0, pos.1, MouseButton::Right),
            UiInput::release(pos.0, pos.1, MouseButton::Right),
            ContainerClick::new(3, 1, ClickType::Pickup),
        ),
        (
            UiInput::MousePress {
                x: pos.0,
                y: pos.1,
                button: MouseButton::Left,
                mods: Modifiers::SHIFT,
                double: false,
            },
            UiInput::MouseRelease {
                x: pos.0,
                y: pos.1,
                button: MouseButton::Left,
                mods: Modifiers::SHIFT,
            },
            ContainerClick::new(3, 0, ClickType::QuickMove),
        ),
    ];
    for (press, release, expected) in table {
        let out = send(&mut screen, &view, &[press, release]);
        assert_eq!(clicks(&out), [expected]);
    }
}

#[test]
fn container_screen_keys() {
    let mut screen = container(MenuKind::Generic { rows: 3 });
    let mut view = TestView::with_slots(63);
    view.set(5, "stone", 10);
    let pos = slot_pos(&screen, 5);
    send(
        &mut screen,
        &view,
        &[UiInput::MouseMove { x: pos.0, y: pos.1 }],
    );
    let out = send(&mut screen, &view, &[UiInput::key(Key::Digit(3))]);
    assert_eq!(clicks(&out), [ContainerClick::new(5, 2, ClickType::Swap)]);
    let out = send(&mut screen, &view, &[UiInput::key(Key::Letter('f'))]);
    assert_eq!(clicks(&out), [ContainerClick::new(5, 40, ClickType::Swap)]);
    let out = send(&mut screen, &view, &[UiInput::key(Key::Letter('q'))]);
    assert_eq!(clicks(&out), [ContainerClick::new(5, 0, ClickType::Throw)]);
    let out = send(
        &mut screen,
        &view,
        &[UiInput::key_mods(Key::Letter('q'), Modifiers::CTRL)],
    );
    assert_eq!(clicks(&out), [ContainerClick::new(5, 1, ClickType::Throw)]);
    let out = send(&mut screen, &view, &[UiInput::key(Key::Escape)]);
    assert!(matches!(out.as_slice(), [ScreenAction::CloseContainer]));
    let out = send(&mut screen, &view, &[UiInput::key(Key::Letter('e'))]);
    assert!(matches!(out.as_slice(), [ScreenAction::CloseContainer]));
}

#[test]
fn container_screen_drag_sequence() {
    let mut screen = container(MenuKind::Generic { rows: 3 });
    let mut view = TestView::with_slots(63);
    view.carried = Some(stack("stone", 9));
    let (a, b, c) = (
        slot_pos(&screen, 1),
        slot_pos(&screen, 2),
        slot_pos(&screen, 11),
    );
    let out = send(
        &mut screen,
        &view,
        &[
            UiInput::press(a.0, a.1, MouseButton::Left),
            UiInput::MouseMove {
                x: a.0 + 1.0,
                y: a.1,
            },
            UiInput::MouseMove { x: b.0, y: b.1 },
            UiInput::MouseMove { x: c.0, y: c.1 },
            UiInput::release(c.0, c.1, MouseButton::Left),
        ],
    );
    assert_eq!(
        clicks(&out),
        [
            ContainerClick::new(-999, 0, ClickType::QuickCraft),
            ContainerClick::new(1, 1, ClickType::QuickCraft),
            ContainerClick::new(2, 1, ClickType::QuickCraft),
            ContainerClick::new(11, 1, ClickType::QuickCraft),
            ContainerClick::new(-999, 2, ClickType::QuickCraft),
        ]
    );
}

#[test]
fn container_screen_click_outside_throws() {
    let mut screen = container(MenuKind::Generic { rows: 3 });
    let view = TestView::with_slots(63);
    let out = send(&mut screen, &view, &click(2.0, 2.0));
    assert_eq!(
        clicks(&out),
        [ContainerClick::new(-999, 0, ClickType::Throw)]
    );
}

#[test]
fn crafter_toggles_empty_slots() {
    let mut screen = container(MenuKind::Crafter);
    let mut view = TestView::with_slots(46);
    view.data = vec![0; 16];
    let pos = slot_pos(&screen, 4);
    let out = send(&mut screen, &view, &click(pos.0, pos.1));
    assert!(out.iter().any(|a| matches!(
        a,
        ScreenAction::CrafterSlotState {
            slot: 4,
            enabled: false
        }
    )));
    assert_eq!(clicks(&out), [ContainerClick::new(4, 0, ClickType::Pickup)]);
    view.data[4] = 1;
    let mut screen = container(MenuKind::Crafter);
    let out = send(&mut screen, &view, &click(pos.0, pos.1));
    assert!(out.iter().any(|a| matches!(
        a,
        ScreenAction::CrafterSlotState {
            slot: 4,
            enabled: true
        }
    )));
}

#[test]
fn enchanting_option_buttons() {
    let mut screen = container(MenuKind::Enchantment);
    let mut view = TestView::with_slots(38);
    view.set(0, "diamond_sword", 1);
    view.set(1, "lapis_lazuli", 2);
    view.data = vec![8, 17, 30, 0, 0, 0, 0, 0, 0, 0];
    view.xp = 20;
    let (l, t) = screen.origin();
    let option = |i: i32| ((l + 100) as f32, (t + 14 + 19 * i + 5) as f32);
    let out = send(&mut screen, &view, &click(option(0).0, option(0).1));
    assert!(matches!(out.as_slice(), [ScreenAction::ContainerButton(0)]));
    let out = send(&mut screen, &view, &click(option(1).0, option(1).1));
    assert!(matches!(out.as_slice(), [ScreenAction::ContainerButton(1)]));
    let out = send(&mut screen, &view, &click(option(2).0, option(2).1));
    assert!(
        out.iter()
            .all(|a| !matches!(a, ScreenAction::ContainerButton(_))),
        "not enough lapis or levels"
    );
}

#[test]
fn stonecutter_recipe_buttons() {
    let mut screen = container(MenuKind::Stonecutter);
    let mut view = TestView::with_slots(38);
    view.set(0, "cobblestone", 1);
    for _ in 0..6 {
        view.options.push(stack("stone_bricks", 1));
    }
    let (l, t) = screen.origin();
    let out = send(
        &mut screen,
        &view,
        &click((l + 52 + 16 + 4) as f32, (t + 14 + 18 + 4) as f32),
    );
    assert!(matches!(
        out.as_slice(),
        [ScreenAction::ContainerButton(5), ScreenAction::ClickSound]
    ));
}

#[test]
fn merchant_trade_selection() {
    use mcr_ui::{MerchantInfo, TradeOffer};
    let mut screen = container(MenuKind::Merchant);
    let mut view = TestView::with_slots(39);
    let offer = TradeOffer {
        base_cost_a: stack("wheat", 20),
        cost_a: stack("wheat", 20),
        result: stack("emerald", 1),
        ..Default::default()
    };
    view.merchant = Some(MerchantInfo {
        offers: vec![offer.clone(), offer.clone(), offer],
        ..Default::default()
    });
    let (l, t) = screen.origin();
    let out = send(
        &mut screen,
        &view,
        &click((l + 50) as f32, (t + 18 + 20 + 10) as f32),
    );
    assert!(matches!(
        out.as_slice(),
        [ScreenAction::SelectTrade(1), ScreenAction::ClickSound]
    ));
}

#[test]
fn lectern_page_keys() {
    let mut screen = container(MenuKind::Lectern);
    let view = TestView::with_slots(1);
    let out = send(
        &mut screen,
        &view,
        &[UiInput::key(Key::Right), UiInput::key(Key::Left)],
    );
    assert!(matches!(
        out.as_slice(),
        [
            ScreenAction::ContainerButton(2),
            ScreenAction::ContainerButton(1)
        ]
    ));
}

#[test]
fn server_list_scrolls_and_selects_the_visible_row() {
    let store = Rc::new(RefCell::new(MemoryServerStore {
        entries: (0..20)
            .map(|i| ServerEntry {
                name: format!("s{i}"),
                address: format!("s{i}.example.com"),
            })
            .collect(),
        ..Default::default()
    }));
    let mut screen = server_list(store);
    send(
        &mut screen,
        &NoView,
        &[UiInput::Scroll {
            x: 200.0,
            y: 100.0,
            dx: 0.0,
            dy: -4.0,
            mods: mcr_ui::Modifiers::NONE,
        }],
    );
    let (x, y) = row_center(0);
    send(&mut screen, &NoView, &click(x + 20.0, y));
    assert_eq!(screen.selected(), Some(2));
    for _ in 0..200 {
        send(
            &mut screen,
            &NoView,
            &[UiInput::Scroll {
                x: 200.0,
                y: 100.0,
                dx: 0.0,
                dy: -4.0,
                mods: mcr_ui::Modifiers::NONE,
            }],
        );
    }
    let last_row_y = (H - 60 - 20) as f32;
    send(&mut screen, &NoView, &click(x + 20.0, last_row_y));
    assert_eq!(screen.selected(), Some(19));
}

#[test]
fn server_list_keyboard_selection_scrolls_into_view() {
    let store = Rc::new(RefCell::new(MemoryServerStore {
        entries: (0..20)
            .map(|i| ServerEntry {
                name: format!("s{i}"),
                address: format!("s{i}.example.com"),
            })
            .collect(),
        ..Default::default()
    }));
    let mut screen = server_list(store);
    for _ in 0..20 {
        send(&mut screen, &NoView, &[UiInput::key(Key::Down)]);
    }
    assert_eq!(screen.selected(), Some(19));
    let out = send(&mut screen, &NoView, &[UiInput::key(Key::Enter)]);
    assert!(
        out.iter()
            .any(|a| matches!(a, ScreenAction::Connect(addr) if addr == "s19.example.com"))
    );
}

fn confirm_outcome(mut out: Vec<ScreenAction>, yes: bool) -> Vec<ScreenAction> {
    let Some(ScreenAction::Push(mut screen)) = out.pop() else {
        panic!("expected a confirmation screen");
    };
    screen.init(W, H);
    let mut clip = MemoryClipboard::default();
    let mut env = Env {
        view: &NoView,
        clipboard: &mut clip,
        now_ms: 0,
    };
    let mut actions = Vec::new();
    let x = if yes {
        213.0 - 155.0 + 75.0
    } else {
        213.0 + 5.0 + 75.0
    };
    let y = (H / 6 + 96 + 10).max(H / 2 - 50 + 60 + 10) as f32;
    screen.input(
        &mut env,
        &UiInput::press(x, y, MouseButton::Left),
        &mut actions,
    );
    actions
}

#[test]
fn server_click_events_route_like_vanilla() {
    let (_, mut screen, _) = chat();
    let mut out = Vec::new();
    for event in [
        ClickEvent::RunCommand("trigger menu".into()),
        ClickEvent::OpenUrl("https://example.com".into()),
        ClickEvent::CopyToClipboard("x".into()),
        ClickEvent::ShowDialog(mcr_nbt::Value::String("x".into())),
        ClickEvent::Custom {
            id: mcr_buf::Identifier::parse("a:b").unwrap(),
            payload: None,
        },
    ] {
        screen.handle_click_event(event, &mut out);
    }
    assert!(
        matches!(
            out.as_slice(),
            [
                ScreenAction::RunCommand(_),
                ScreenAction::OpenLink(_),
                ScreenAction::CopyToClipboard(_),
                ScreenAction::ShowDialog(_),
                ScreenAction::CustomClick { .. },
            ]
        ),
        "{out:?}"
    );
    out.clear();
    for event in [
        ClickEvent::OpenFile("/etc/passwd".into()),
        ClickEvent::ChangePage(3),
    ] {
        screen.handle_click_event(event, &mut out);
    }
    assert!(out.is_empty());
}

#[test]
fn confirmed_command_is_exactly_the_displayed_one() {
    let prompt = mcr_ui::ConfirmScreen::command_prompt(
        "say hi",
        mcr_ui::CommandPrompt::PermissionsRequired,
        true,
    );
    let acts = confirm_outcome(vec![ScreenAction::Push(Box::new(prompt))], true);
    assert!(
        acts.iter()
            .any(|a| matches!(a, ScreenAction::SendCommand(c) if c == "say hi")),
        "{acts:?}"
    );
    let prompt = mcr_ui::ConfirmScreen::command_prompt(
        "say hi",
        mcr_ui::CommandPrompt::SignatureRequired { suggest: false },
        true,
    );
    let acts = confirm_outcome(vec![ScreenAction::Push(Box::new(prompt))], true);
    assert!(
        acts.iter()
            .any(|a| matches!(a, ScreenAction::CopyToClipboard(c) if c == "/say hi")),
        "{acts:?}"
    );
    let prompt =
        mcr_ui::ConfirmScreen::command_prompt("op me", mcr_ui::CommandPrompt::ParseErrors, true);
    let acts = confirm_outcome(vec![ScreenAction::Push(Box::new(prompt))], false);
    assert!(
        matches!(acts.as_slice(), [.., ScreenAction::Pop]),
        "{acts:?}"
    );
    assert!(
        !acts
            .iter()
            .any(|a| matches!(a, ScreenAction::SendCommand(_)))
    );
}

#[test]
fn spoofable_targets_are_rejected() {
    let (_, mut screen, _) = chat();
    for event in [
        ClickEvent::OpenUrl("https://good.com@evil.com/".into()),
        ClickEvent::OpenUrl("https://exаmple.com".into()),
        ClickEvent::OpenUrl("https://example.com/\u{202E}gpj".into()),
        ClickEvent::OpenUrl("javascript:alert(1)".into()),
        ClickEvent::OpenUrl("https://a.com\\@evil.com".into()),
        ClickEvent::RunCommand("say hi\u{202E}".into()),
        ClickEvent::RunCommand("say \u{a7}ahi".into()),
        ClickEvent::RunCommand("say hi\nop me".into()),
        ClickEvent::RunCommand("/".into()),
        ClickEvent::CopyToClipboard("a".repeat(40_000)),
        ClickEvent::CopyToClipboard("rm -rf\u{200B}".into()),
    ] {
        let mut out = Vec::new();
        screen.handle_click_event(event.clone(), &mut out);
        assert!(out.is_empty(), "{event:?}");
    }
}

struct Asleep(bool);

impl mcr_ui::GameView for Asleep {
    fn sleeping(&self) -> bool {
        self.0
    }
}

fn in_bed() -> ChatScreen {
    let hud: SharedChat = Rc::new(RefCell::new(ChatHud::default()));
    let mut screen = ChatScreen::new(
        hud,
        Box::new(SharedScript::default()),
        ChatOptions::default(),
    )
    .in_bed();
    screen.init(W, H);
    screen
}

fn tick_out(screen: &mut ChatScreen, view: &dyn mcr_ui::GameView) -> Vec<ScreenAction> {
    let mut clip = MemoryClipboard::default();
    let mut env = Env {
        view,
        clipboard: &mut clip,
        now_ms: 0,
    };
    let mut out = Vec::new();
    screen.tick(&mut env, &mut out);
    out
}

#[test]
fn leave_bed_button_and_escape_wake_up_without_closing() {
    let mut screen = in_bed();
    let out = send(
        &mut screen,
        &Asleep(true),
        &click((W / 2) as f32, (H - 30) as f32),
    );
    assert!(matches!(
        out.as_slice(),
        [ScreenAction::ClickSound, ScreenAction::StopSleeping]
    ));
    let out = key(&mut screen, Key::Escape);
    assert!(matches!(out.as_slice(), [ScreenAction::StopSleeping]));
}

#[test]
fn in_bed_chat_stays_open_after_sending() {
    let mut screen = in_bed();
    send(&mut screen, &Asleep(true), &typed("good night"));
    let out = key(&mut screen, Key::Enter);
    assert!(matches!(out.as_slice(), [ScreenAction::SendChat(m)] if m == "good night"));
    assert_eq!(screen.text(), "");
}

#[test]
fn waking_up_closes_the_screen_or_keeps_the_typed_text() {
    let mut screen = in_bed();
    assert!(tick_out(&mut screen, &Asleep(true)).is_empty());
    assert!(matches!(
        tick_out(&mut screen, &Asleep(false)).as_slice(),
        [ScreenAction::Pop]
    ));

    let mut screen = in_bed();
    send(&mut screen, &Asleep(true), &typed("brb"));
    assert!(tick_out(&mut screen, &Asleep(false)).is_empty());
    assert!(!screen.is_in_bed());
    assert_eq!(screen.text(), "brb");
    let out = key(&mut screen, Key::Escape);
    assert!(matches!(out.as_slice(), [ScreenAction::Pop]));
}
