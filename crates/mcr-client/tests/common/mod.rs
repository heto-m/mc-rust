#![allow(dead_code)]
use std::fs::OpenOptions;
use std::io::Write;
use std::time::{Duration, Instant};

use mcr_buf::BlockPos;
use mcr_client::{Client, ClientConfig, ClientEvent, PlayerInput, WireChoice};

impl Live {
    pub fn connect(name: &str) -> Live {
        let mut cfg = ClientConfig::default().with_addr(&env("MCR_LIVE_ADDR"));
        cfg.name = name.into();
        cfg.view_distance = 6;
        any_version(&mut cfg);
        let client = Client::connect(cfg).expect("join");
        Live {
            client,
            console: env("MCR_LIVE_CONSOLE"),
            log: env("MCR_LIVE_LOG"),
            events: Vec::new(),
            yaw: 0.0,
        }
    }
}

pub struct Live {
    pub client: Client,
    pub console: String,
    pub log: String,
    pub events: Vec<ClientEvent>,
    pub yaw: f32,
}

/// `MCR_LIVE_ANY_VERSION` joins older servers through their translator.
pub fn any_version(cfg: &mut ClientConfig) {
    if std::env::var_os("MCR_LIVE_ANY_VERSION").is_some() {
        cfg.wire = WireChoice::Auto;
    }
}

pub fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("set {name}"))
}

impl Live {
    pub fn console(&self, line: &str) {
        let mut f = OpenOptions::new().append(true).open(&self.console).unwrap();
        writeln!(f, "{line}").unwrap();
    }

    pub fn log_text(&self) -> String {
        std::fs::read_to_string(&self.log).unwrap_or_default()
    }

    pub fn run(&mut self, ticks: u32, input: impl Fn(u32) -> PlayerInput) {
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

    pub fn idle(&mut self, ticks: u32) {
        self.run(ticks, |_| PlayerInput::default());
    }

    pub fn until(&mut self, what: &str, max_ticks: u32, mut done: impl FnMut(&Live) -> bool) {
        for _ in 0..max_ticks {
            if done(self) {
                return;
            }
            self.idle(1);
        }
        panic!("timed out waiting for {what}");
    }

    pub fn teleports(&self) -> u32 {
        self.client.state.player.teleports_received
    }

    pub fn pos(&self) -> [f64; 3] {
        self.client.state.player.physics.pos
    }

    pub fn block(&self, p: BlockPos) -> u16 {
        self.client
            .state
            .chunks
            .block(p.x, p.y, p.z)
            .map_or(0, |b| b.0)
    }

    pub fn server_says(&mut self, command: &str, needle: &str) -> bool {
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

pub fn walk(yaw_sprint: bool) -> impl Fn(u32) -> PlayerInput {
    move |_| PlayerInput {
        forward: true,
        sprint: yaw_sprint,
        ..PlayerInput::default()
    }
}
