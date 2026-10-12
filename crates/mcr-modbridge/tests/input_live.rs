//! Opt-in: scripted input driving Java's screens while joined to a vanilla 26.3 server.
//! `MCR_JVM_TESTS=1 MCR_LIVE_ADDR=localhost:25567 MCR_LIVE_CONSOLE=<file the server's stdin tails>
//!  [MCR_LIVE_LOG=<server log, default run/server-26.3/logs/latest.log>]
//!  cargo test -p mcr-modbridge --test input_live -- --nocapture`

mod gpu_common;

use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use gpu_common::compose::*;
use gpu_common::input::*;
use gpu_common::{config, repo};
use mcr_client::{Client, ClientConfig};
use mcr_modbridge::ShutdownOutcome;
use mcr_modbridge::input::NativeKind;

const USERNAME: &str = "Typist";
const TITLE: &str = "net.minecraft.client.gui.screens.TitleScreen";
const CHAT_LINE: &str = "mcr input check";
const KEY_E: u32 = 8;
const KEY_T: u32 = 23;
const KEY_K: u32 = 14;

fn console(line: &str) {
    let path = std::env::var("MCR_LIVE_CONSOLE").expect("set MCR_LIVE_CONSOLE");
    let mut f = std::fs::OpenOptions::new().append(true).open(path).unwrap();
    writeln!(f, "{line}").unwrap();
}

/// The server log from `from` bytes on, so earlier runs against the same server never match.
struct ServerLog {
    path: PathBuf,
    from: usize,
}

impl ServerLog {
    fn open() -> Self {
        let path = std::env::var_os("MCR_LIVE_LOG")
            .map(PathBuf::from)
            .unwrap_or_else(|| repo().join("run/server-26.3/logs/latest.log"));
        let from = std::fs::read(&path).map_or(0, |b| b.len());
        Self { path, from }
    }

    fn contains(&self, needle: &str) -> bool {
        std::fs::read(&self.path)
            .ok()
            .and_then(|b| {
                b.get(self.from..)
                    .map(|s| String::from_utf8_lossy(s).into_owned())
            })
            .is_some_and(|s| s.contains(needle))
    }
}

/// Hotbar slot 0 of the survival inventory, in window pixels (recipe book closed).
fn hotbar_slot_0(l: &Live) -> (f64, f64) {
    let scale = l.input.java().gui_scale.max(1);
    let (w, h) = l.rig.renderer.size();
    let (gw, gh) = (w.div_ceil(scale), h.div_ceil(scale));
    let left = (gw - 176) / 2;
    let top = (gh - 166) / 2;
    (
        f64::from((left + 8 + 8) * scale),
        f64::from((top + 142 + 8) * scale),
    )
}

#[test]
fn java_screens_take_scripted_input_on_a_live_server() {
    let (Ok("1"), Ok(addr)) = (
        std::env::var("MCR_JVM_TESTS").as_deref(),
        std::env::var("MCR_LIVE_ADDR"),
    ) else {
        eprintln!("skipped: set MCR_JVM_TESTS=1 and MCR_LIVE_ADDR (see the file header)");
        return;
    };
    let (host, port) = addr.rsplit_once(':').expect("host:port");
    let port: u16 = port.parse().expect("port");
    let mut cfg = config("input-live", &["-Dmcr.bridge.probeInput=true".into()]);
    (cfg.width, cfg.height) = (1280, 720);
    cfg.username = USERNAME.into();
    cfg.uuid = mcr_auth::offline::offline_uuid(USERNAME)
        .unwrap()
        .hyphenated()
        .to_string();
    let Some(mut rig) = Rig::start(cfg) else {
        return;
    };
    rig.until("the title screen", |r| r.screen == TITLE && r.loaded());
    let log = ServerLog::open();

    let link = rig.bridge.net();
    let mut ccfg = ClientConfig::default().with_addr(&format!("{host}:{port}"));
    ccfg.name = USERNAME.into();
    let client = Client::connect_hooked(ccfg, link.hooks(host, port)).expect("join");
    let mut l = Live::new(rig);
    l.client = Some(client);
    l.until("java in game", |l| {
        l.rig.screen.is_empty() && l.rig.frames.lock().java().loaded
    });
    console(&format!("gamemode survival {USERNAME}"));
    console(&format!("clear {USERNAME}"));
    console(&format!("give {USERNAME} stone 16"));
    l.run_for(Duration::from_secs(2));
    l.until("the mouse grabbed in game", |l| l.input.java().grabbed);

    l.tap(KEY_E);
    l.wait_screen("InventoryScreen");
    assert!(!l.input.java().grabbed, "a screen frees the mouse");
    let (x, y) = hotbar_slot_0(&l);
    l.input.key(LEFT_SHIFT, None, true, false);
    l.settle(2);
    l.click_at(x, y, LEFT);
    l.input.key(LEFT_SHIFT, None, false, false);
    l.settle(2);
    l.tap(ESCAPE);
    l.until("the inventory closed", |l| l.rig.screen.is_empty());
    l.run_for(Duration::from_millis(500));
    console(&format!(
        "execute if items entity {USERNAME} inventory.0 minecraft:stone"
    ));
    l.until("stone moved to the main inventory", |_| {
        log.contains("Test passed. Count: 16")
    });

    l.tap(KEY_T);
    l.wait_screen("ChatScreen");
    l.until("text input for chat", |l| l.input.java().text_input);
    l.type_text(CHAT_LINE);
    l.tap(ENTER);
    l.until("the chat line on the server", |_| log.contains(CHAT_LINE));
    l.until("chat closed", |l| l.rig.screen.is_empty());

    l.tap(KEY_K);
    l.wait_info("testmod_key", "1");

    l.tap(ESCAPE);
    l.wait_screen("PauseScreen");
    assert!(!l.input.java().grabbed);
    l.click(l.button("mcr"));
    l.run_for(Duration::from_millis(200));
    assert_eq!(l.input.take_native_request(), Some(NativeKind::Menu));
    l.tap(ESCAPE);
    l.until("back in game", |l| {
        l.rig.screen.is_empty() && l.input.java().grabbed
    });

    let stats = l.rig.bridge.net().stats();
    eprintln!("net: {stats:?}");
    assert!(stats.forwarded > 0, "container clicks were forwarded");
    assert!(stats.chats >= 1);
    assert_eq!(l.input.dropped(), 0);
    assert_eq!(l.rig.detached, None, "the bridge detached");
    l.client = None;
    let outcome = l.rig.bridge.shutdown(mcr_modbridge::SHUTDOWN_TIMEOUT);
    assert!(!matches!(outcome, ShutdownOutcome::Crashed), "{outcome:?}");
}
