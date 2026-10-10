#![allow(clippy::field_reassign_with_default)]
//! Run manually against a vanilla server:
//! `MCR_LIVE_ADDR=localhost:25580 MCR_LIVE_CONSOLE=<file the server's stdin tails> \
//!  MCR_LIVE_LOG=<server log> cargo test -p mcr-client --test live -- --ignored --nocapture`

use std::fs::OpenOptions;
use std::io::Write;
use std::time::{Duration, Instant};

use mcr_buf::BlockPos;
use mcr_client::{Client, ClientConfig, ClientEvent, PlayerInput};
use mcr_protocol::play::serverbound::Facing;
use mcr_protocol::types::ItemStack;

struct Live {
    client: Client,
    console: String,
    log: String,
    events: Vec<ClientEvent>,
    yaw: f32,
}

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("set {name}"))
}

impl Live {
    fn console(&self, line: &str) {
        let mut f = OpenOptions::new().append(true).open(&self.console).unwrap();
        writeln!(f, "{line}").unwrap();
    }

    fn log_text(&self) -> String {
        std::fs::read_to_string(&self.log).unwrap_or_default()
    }

    fn run(&mut self, ticks: u32, input: impl Fn(u32) -> PlayerInput) {
        let mut next = Instant::now();
        for t in 0..ticks {
            let mut keys = input(t);
            keys.yaw = self.yaw;
            self.client.tick(&keys).expect("tick");
            self.events.extend(self.client.drain_events());
            next += Duration::from_millis(50);
            if let Some(d) = next.checked_duration_since(Instant::now()) {
                std::thread::sleep(d);
            }
        }
    }

    fn idle(&mut self, ticks: u32) {
        self.run(ticks, |_| PlayerInput::default());
    }

    fn until(&mut self, what: &str, max_ticks: u32, mut done: impl FnMut(&Live) -> bool) {
        for _ in 0..max_ticks {
            if done(self) {
                return;
            }
            self.idle(1);
        }
        panic!("timed out waiting for {what}");
    }

    fn teleports(&self) -> u32 {
        self.client.state.player.teleports_received
    }

    fn pos(&self) -> [f64; 3] {
        self.client.state.player.physics.pos
    }

    fn block(&self, p: BlockPos) -> u16 {
        self.client
            .state
            .chunks
            .block(p.x, p.y, p.z)
            .map_or(0, |b| b.0)
    }

    fn server_says(&mut self, command: &str, needle: &str) -> bool {
        let before = self.log_text().len();
        self.console(command);
        for _ in 0..40 {
            std::thread::sleep(Duration::from_millis(100));
            if self.log_text()[before.min(self.log_text().len())..].contains(needle) {
                return true;
            }
        }
        false
    }
}

fn walk(yaw_sprint: bool) -> impl Fn(u32) -> PlayerInput {
    move |_| PlayerInput {
        forward: true,
        sprint: yaw_sprint,
        ..PlayerInput::default()
    }
}

#[test]
#[ignore = "needs a running vanilla server"]
fn live_session() {
    let mut cfg = ClientConfig::default().with_addr(&env("MCR_LIVE_ADDR"));
    cfg.name = "Bot".into();
    cfg.view_distance = 10;
    let client = Client::connect(cfg).expect("join");
    let mut live = Live {
        client,
        console: env("MCR_LIVE_CONSOLE"),
        log: env("MCR_LIVE_LOG"),
        events: Vec::new(),
        yaw: 0.0,
    };
    live.console("op Bot");
    live.console("gamemode creative Bot");

    live.until("spawn and loaded", 400, |l| l.client.state.player.loaded);
    live.until("chunks", 600, |l| l.client.state.chunks.len() >= 300);
    live.idle(100);
    let chunks = live.client.state.chunks.len();
    println!("chunks after join: {chunks}");
    assert!(chunks >= 300, "view distance 10 should load ~317 chunks");
    let baseline = live.teleports();
    assert_eq!(baseline, 1, "exactly the initial teleport");

    // Physics validation: walk, sprint, turn and jump on real terrain.
    for yaw in [0.0, 90.0, 180.0, 270.0] {
        live.yaw = yaw;
        live.run(40, walk(true));
        live.run(1, |_| PlayerInput {
            forward: true,
            jump: true,
            ..PlayerInput::default()
        });
        live.run(20, walk(false));
    }
    live.idle(40);
    println!("pos after walking {:?}", live.pos());
    assert_eq!(
        live.teleports(),
        baseline,
        "server must not correct our movement"
    );
    let log = live.log_text();
    assert!(!log.contains("moved wrongly"), "server rejected movement");
    assert!(!log.contains("moved too quickly"));

    // Chat roundtrip and commands.
    live.client.send_chat("hello from mcr").unwrap();
    live.until("chat echo", 100, |l| {
        l.events.iter().any(|e| matches!(e, ClientEvent::Chat { message, .. } if message.to_plain_string().contains("hello from mcr")))
    });
    live.client.send_command("time set 6000").unwrap();
    live.until("time set", 100, |l| matches!(l.client.state.world.day_time(), Some(t) if t % 24000 >= 6000 && t % 24000 < 6400));

    // Break and place blocks (creative).
    live.idle(20);
    let p = live.pos();
    let (bx, by, bz) = (
        p[0].floor() as i32,
        p[1].floor() as i32,
        p[2].floor() as i32,
    );
    let target = BlockPos::new(bx + 2, by - 1, bz);
    let before = live.block(target);
    assert_ne!(before, 0, "expected solid ground to break");
    live.client.start_digging(target, Facing::Up).unwrap();
    live.until("block break", 60, |l| l.block(target) == 0);
    assert!(live.server_says(
        &format!(
            "execute if block {} {} {} air",
            target.x, target.y, target.z
        ),
        "Test passed"
    ));

    let stone = live
        .client
        .state
        .data
        .item_by_name("minecraft:stone")
        .unwrap() as i32;
    live.client
        .set_creative_slot(36, ItemStack::new(stone, 64))
        .unwrap();
    live.client.select_hotbar(0).unwrap();
    live.idle(10);
    let below = BlockPos::new(target.x, target.y - 1, target.z);
    live.client.place_block(below, Facing::Up).unwrap();
    live.until("block place", 60, |l| l.block(target) != 0);
    assert!(live.server_says(
        &format!(
            "execute if block {} {} {} stone",
            target.x, target.y, target.z
        ),
        "Test passed"
    ));

    // Inventory view.
    live.client.open_inventory().unwrap();
    live.idle(10);
    assert_eq!(live.client.state.inventory.hotbar(0).item, stone);

    // Death and respawn.
    live.console("gamemode survival Bot");
    live.idle(20);
    live.console("kill Bot");
    live.until("death", 100, |l| l.client.state.player.dead);
    live.client.respawn().unwrap();
    live.until("respawn", 100, |l| {
        !l.client.state.player.dead && l.client.state.player.loaded
    });
    live.idle(60);
    assert!(
        live.events
            .iter()
            .any(|e| matches!(e, ClientEvent::Respawned))
    );
    println!("teleports after respawn: {}", live.teleports());

    // Transfer to itself.
    live.console("gamemode creative Bot");
    let addr = env("MCR_LIVE_ADDR");
    live.console(&format!(
        "transfer {} {} Bot",
        addr.split(':').next().unwrap(),
        addr.split(':').nth(1).unwrap()
    ));
    for _ in 0..100 {
        if live.client.is_closed() {
            break;
        }
        let _ = live.client.tick(&PlayerInput::default());
        live.events.extend(live.client.drain_events());
        std::thread::sleep(Duration::from_millis(50));
    }
    let (host, port) = live.client.take_transfer().expect("transfer packet");
    println!("transfer to {host}:{port}");

    let mut cfg = ClientConfig::default();
    cfg.host = host;
    cfg.port = port;
    cfg.transfer = true;
    cfg.view_distance = 10;
    let client = Client::connect(cfg).expect("rejoin after transfer");
    live.client = client;
    live.until("loaded after transfer", 400, |l| {
        l.client.state.player.loaded
    });
    live.idle(200);
    println!("done; chunks {}", live.client.state.chunks.len());
}

#[test]
#[ignore = "needs a running vanilla server"]
fn live_long_idle() {
    let mut cfg = ClientConfig::default().with_addr(&env("MCR_LIVE_ADDR"));
    cfg.name = "Idler".into();
    cfg.view_distance = 10;
    let client = Client::connect(cfg).expect("join");
    let mut live = Live {
        client,
        console: env("MCR_LIVE_CONSOLE"),
        log: env("MCR_LIVE_LOG"),
        events: Vec::new(),
        yaw: 0.0,
    };
    live.idle(20 * 150);
    assert!(!live.client.is_closed());
    assert_eq!(live.teleports(), 1);
}
