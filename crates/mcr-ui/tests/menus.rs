mod common;

use std::cell::RefCell;
use std::rc::Rc;

use common::harness::{Harness, SIZE, TestView, send, typed};
use mcr_protocol::types::text::{Arg, ClickEvent, Color, Component, Style};
use mcr_ui::{
    ChatHud, ChatOptions, ChatScreen, CommandSuggester, ConductScreen, ConnectingScreen,
    DeathScreen, DirectConnectScreen, DisconnectedScreen, EditServerScreen, MemoryServerStore,
    NoView, OnlineStatus, PauseScreen, Screen, ServerEntry, ServerListScreen, ServerStatus,
    SharedChat, Suggestion, SuggestionSet, TitleScreen, UiInput,
};

struct Canned(Option<SuggestionSet>);

impl CommandSuggester for Canned {
    fn request(&mut self, _text: &str, _cursor: usize) {}

    fn poll(&mut self) -> Option<SuggestionSet> {
        self.0.take()
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

fn chat_with_history(h: &mut Harness) -> SharedChat {
    let chat: SharedChat = Rc::new(RefCell::new(ChatHud::default()));
    let options = ChatOptions::default();
    let mut hud = chat.borrow_mut();
    for (i, line) in [
        "§e<Steve> hello there",
        "§7Alex joined the game",
        "<Alex> anyone want to trade?",
        "§aServer: welcome to the test server",
    ]
    .into_iter()
    .enumerate()
    {
        hud.add_text(&mut h.ui, line, i as u32, &options);
    }
    let link = Component::text("[click for the wiki]").styled(Style {
        click_event: Some(Box::new(ClickEvent::OpenUrl(
            "https://minecraft.wiki".into(),
        ))),
        underlined: Some(true),
        color: Some(Color::Rgb(0x55FFFF)),
        ..Default::default()
    });
    hud.add(&mut h.ui, &link, 5, &options);
    drop(hud);
    chat
}

#[test]
fn chat_suggestions_shot() {
    let Some(mut h) = Harness::new() else { return };
    let chat = chat_with_history(&mut h);
    let set = SuggestionSet {
        suggestions: ["gamemode", "gamerule", "give", "gamemode"]
            .iter()
            .map(|t| suggestion(1, 3, t))
            .collect(),
        usage: Vec::new(),
    };
    let mut screen = ChatScreen::new(chat, Box::new(Canned(Some(set))), ChatOptions::default());
    screen.init(427, 240);
    send(&mut screen, &NoView, &typed("/ga"));
    h.shot("screen_chat.png", SIZE, &mut screen, &NoView, (30.0, 30.0));
    assert!(screen.popup_len() > 0);
}

#[test]
fn death_shot() {
    let Some(mut h) = Harness::new() else { return };
    let mut screen = DeathScreen::new(
        Some(Component::translatable(
            "death.attack.fall",
            vec![Arg::Component(Component::text("Steve"))],
        )),
        false,
    );
    let view = TestView {
        score: 137,
        ..TestView::default()
    };
    screen.init(427, 240);
    let mut clip = mcr_ui::MemoryClipboard::default();
    let mut env = mcr_ui::Env {
        view: &view,
        clipboard: &mut clip,
        now_ms: 0,
    };
    let mut out = Vec::new();
    for _ in 0..20 {
        screen.tick(&mut env, &mut out);
    }
    h.shot_inited("screen_death.png", SIZE, &mut screen, &view, (213.0, 100.0));
}

fn sample_store() -> Rc<RefCell<MemoryServerStore>> {
    Rc::new(RefCell::new(MemoryServerStore {
        entries: vec![
            ServerEntry {
                name: "Hypixel".into(),
                address: "mc.hypixel.net".into(),
            },
            ServerEntry {
                name: "Local test".into(),
                address: "localhost:25565".into(),
            },
            ServerEntry {
                name: "Old server".into(),
                address: "old.example.com".into(),
            },
            ServerEntry {
                name: "Down".into(),
                address: "down.example.com".into(),
            },
        ],
        statuses: vec![
            Some(ServerStatus::Online(Box::new(OnlineStatus {
                motd: Component::text("§aA Minecraft Server §7- join now"),
                players: "1234/20000".into(),
                version: "1.21".into(),
                ping_ms: Some(80),
                compatible: true,
                favicon: None,
            }))),
            Some(ServerStatus::Online(Box::new(OnlineStatus {
                motd: Component::text("Welcome to my local server"),
                players: "0/20".into(),
                version: "26.3".into(),
                ping_ms: Some(450),
                compatible: true,
                favicon: None,
            }))),
            Some(ServerStatus::Online(Box::new(OnlineStatus {
                motd: Component::text("Legacy"),
                players: "5/10".into(),
                version: "1.8.9".into(),
                ping_ms: Some(20),
                compatible: false,
                favicon: None,
            }))),
            Some(ServerStatus::Unreachable {
                message: "§4Can't connect to server".into(),
            }),
        ],
        last_direct: String::new(),
    }))
}

#[test]
fn server_list_shot() {
    let Some(mut h) = Harness::new() else { return };
    let mut screen = ServerListScreen::new(sample_store());
    screen.init(427, 240);
    screen.select(Some(1));
    h.shot(
        "screen_server_list.png",
        SIZE,
        &mut screen,
        &NoView,
        (213.0, 100.0),
    );
}

#[test]
fn small_menu_shots() {
    let Some(mut h) = Harness::new() else { return };
    let store = sample_store();
    h.shot(
        "screen_pause.png",
        SIZE,
        &mut PauseScreen::new(true, false),
        &NoView,
        (213.0, 100.0),
    );
    h.shot(
        "screen_title.png",
        SIZE,
        &mut TitleScreen::new("Minecraft 26.3", store.clone()),
        &NoView,
        (213.0, 100.0),
    );
    h.shot(
        "screen_disconnected.png",
        SIZE,
        &mut DisconnectedScreen::new(Component::text(
            "Kicked for flying is not enabled on this server",
        )),
        &NoView,
        (213.0, 100.0),
    );
    let (mut connecting, _) = ConnectingScreen::with_text("lang:connect.connecting");
    h.shot(
        "screen_connecting.png",
        SIZE,
        &mut connecting,
        &NoView,
        (213.0, 100.0),
    );
    h.shot(
        "screen_direct_connect.png",
        SIZE,
        &mut DirectConnectScreen::new(store.clone()),
        &NoView,
        (213.0, 100.0),
    );
    h.shot(
        "screen_edit_server.png",
        SIZE,
        &mut EditServerScreen::new(store, Some(1)),
        &NoView,
        (213.0, 100.0),
    );
}

#[test]
fn chat_hud_resolves_clicks_on_wrapped_components() {
    let Some(mut h) = Harness::new() else { return };
    let chat = chat_with_history(&mut h);
    let options = ChatOptions {
        focused: true,
        visible_lines: 20,
        ..ChatOptions::default()
    };
    let hud = chat.borrow();
    let bottom = 240 - 40;
    let newest = (bottom - 4) as f32;
    let hit = hud.click_event_at(10.0, newest, 240, &options);
    assert_eq!(
        hit,
        Some(ClickEvent::OpenUrl("https://minecraft.wiki".into()))
    );
    assert_eq!(hud.click_event_at(300.0, newest, 240, &options), None);
    assert_eq!(hud.click_event_at(10.0, newest - 9.0, 240, &options), None);
    assert_eq!(hud.click_event_at(10.0, 100.0, 240, &options), None);
}

#[test]
fn chat_scrolls_and_clamps() {
    let Some(mut h) = Harness::new() else { return };
    let chat: SharedChat = Rc::new(RefCell::new(ChatHud::default()));
    let options = ChatOptions::default();
    for i in 0..30 {
        chat.borrow_mut()
            .add_text(&mut h.ui, &format!("line {i}"), 0, &options);
    }
    let mut screen = ChatScreen::new(chat.clone(), Box::new(mcr_ui::NoSuggestions), options);
    screen.init(427, 240);
    send(&mut screen, &NoView, &[UiInput::key(mcr_ui::Key::PageUp)]);
    assert_eq!(chat.borrow().scroll(), 10);
    send(
        &mut screen,
        &NoView,
        &[
            UiInput::key(mcr_ui::Key::PageUp),
            UiInput::key(mcr_ui::Key::PageUp),
        ],
    );
    assert_eq!(chat.borrow().scroll(), 10);
    send(&mut screen, &NoView, &[UiInput::key(mcr_ui::Key::PageDown)]);
    assert_eq!(chat.borrow().scroll(), 0);
    send(
        &mut screen,
        &NoView,
        &[UiInput::Scroll {
            x: 10.0,
            y: 10.0,
            dx: 0.0,
            dy: 1.0,
        }],
    );
    assert_eq!(chat.borrow().scroll(), 7);
    h.shot(
        "screen_chat_scrolled.png",
        SIZE,
        &mut screen,
        &NoView,
        (30.0, 30.0),
    );
}

#[test]
fn conduct_screen_answers() {
    use mcr_ui::{Key, MouseButton, ScreenAction};
    let text = "Be kind.\n".repeat(80);
    let mut screen = ConductScreen::new(&text, 42);
    screen.init(427, 240);
    let click = |x, y| {
        [
            UiInput::press(x, y, MouseButton::Left),
            UiInput::release(x, y, MouseButton::Left),
        ]
    };
    send(&mut screen, &NoView, &click(40.0, 198.0));
    assert!(screen.remember());
    let out = send(&mut screen, &NoView, &click(80.0, 222.0));
    assert!(out.iter().any(|a| matches!(
        a,
        ScreenAction::ConductAnswer {
            accept: true,
            remember: true,
            hash: 42
        }
    )));
    let out = send(&mut screen, &NoView, &[UiInput::key(Key::Escape)]);
    assert!(out.iter().any(|a| matches!(
        a,
        ScreenAction::ConductAnswer {
            accept: false,
            remember: false,
            ..
        }
    )));
}

/// The "mcr" menu over Java's pause screen: nothing bright shows around its panel at 1280x720, scale 3.
#[test]
fn panel_menu_covers_the_screen_behind_it() {
    let Some(mut h) = Harness::new() else { return };
    let size = (1280, 720);
    let mut screen = mcr_ui::ConfirmScreen::new(
        "mcr",
        "",
        vec![
            ("Options...".into(), Box::new(Vec::new)),
            ("Mods...".into(), Box::new(Vec::new)),
            ("Done".into(), Box::new(Vec::new)),
        ],
    )
    .with_panel();
    let scale = mcr_ui::gui_scale(size.0, size.1, 0);
    assert_eq!(scale, 3);
    screen.init((size.0 / scale) as i32, (size.1 / scale) as i32);
    {
        let mut frame = h.ui.frame(size.0, size.1);
        let mut cx = mcr_ui::RenderCx::new(&mut frame, &mut h.icons, &h.assets, &NoView);
        screen.render(&mut cx);
        cx.finish();
    }
    let img = common::render(&h.gpu, &mut h.renderer, &mut h.ui, size, wgpu::Color::WHITE);
    common::save_png("menu-panel-backdrop.png", size.0, size.1, &img);
    for (x, y) in [
        (20, 20),
        (640, 20),
        (40, 360),
        (1240, 360),
        (640, 700),
        (1260, 700),
    ] {
        let i = ((y * size.0 + x) * 4) as usize;
        assert!(
            img[i..i + 3].iter().all(|&c| c < 40),
            "({x},{y}) shows through: {:?}",
            &img[i..i + 3]
        );
    }
    let gap = ((437 * size.0 + 432) * 4) as usize;
    assert!(
        img[gap..gap + 3].iter().all(|&c| c <= 20),
        "the opaque panel reaches the buttons: {:?}",
        &img[gap..gap + 3]
    );
}
