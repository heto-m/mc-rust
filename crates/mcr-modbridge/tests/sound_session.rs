//! Opt-in sound ownership check against a running vanilla 26.3 server (offline):
//! `MCR_JVM_TESTS=1 MCR_LIVE_ADDR=localhost:25565 MCR_LIVE_CONSOLE=<file the server's stdin tails>
//!  cargo test -p mcr-modbridge --test sound_session -- --nocapture`
//! Rust plays the session while Java mirrors it into an offline mixer. Per phase, every sound event is
//! classified by owner and every sound Java plays is traced: Java must play the server's sounds and its own
//! ambience, but never the local player's movement or mcr's predicted block sounds.

use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use mcr_audio::{Mixer, VOICE_PAUSED, VOICE_PLAYING};
use mcr_client::{Client, ClientConfig, ClientEvent, PlayerInput};
use mcr_modbridge::sound::{SOUND_SLOTS, SoundLink, SoundOwner, VoiceStart, event_owner};
use mcr_modbridge::{Bridge, BridgeConfig, BridgeEvent, LaunchManifest};
use mcr_protocol::play::serverbound::Facing;

const USERNAME: &str = "Bridge";
const RATE: u32 = 48_000;
const BOOT_DEADLINE: Duration = Duration::from_secs(240);
const SETTLE_TICKS: u32 = 30;

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("set {name}"))
}

fn console(line: &str) {
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(env("MCR_LIVE_CONSOLE"))
        .unwrap();
    writeln!(f, "{line}").unwrap();
}

/// A sound Java asked to play, with the local player's position when the trace arrived.
#[derive(Debug, Clone)]
struct JavaPlay {
    id: String,
    pos: [f64; 3],
    player: [f64; 3],
}

impl JavaPlay {
    fn parse(value: &str, player: [f64; 3]) -> Option<Self> {
        let mut it = value.split(' ');
        let id = it.next()?.to_owned();
        let mut pos = [0.0; 3];
        for p in &mut pos {
            *p = it.next()?.parse().ok()?;
        }
        Some(Self { id, pos, player })
    }

    fn near(&self, at: [f64; 3], radius: f64) -> bool {
        self.pos
            .iter()
            .zip(at)
            .map(|(a, b)| (a - b).powi(2))
            .sum::<f64>()
            <= radius * radius
    }

    fn is_movement(&self) -> bool {
        [".step", ".fall", ".swim", ".splash", "amethyst_block.chime"]
            .iter()
            .any(|k| self.id.contains(k))
            && self.near(self.player, 3.0)
    }
}

#[derive(Debug, Default, Clone)]
struct Tally {
    server_sounds: usize,
    mcr_owned: usize,
    java_owned: usize,
    java_plays: Vec<JavaPlay>,
    java_static: Vec<VoiceStart>,
    java_streams: usize,
}

impl Tally {
    fn plays(&self, prefix: &str) -> usize {
        self.java_plays
            .iter()
            .filter(|p| p.id.starts_with(prefix))
            .count()
    }
}

struct Session {
    bridge: Bridge,
    client: Client,
    sound: SoundLink,
    tally: Tally,
}

impl Session {
    fn tick(&mut self, input: &PlayerInput) {
        let start = Instant::now();
        self.client.tick(input).expect("tick");
        let events: Vec<ClientEvent> = self.client.drain_events().collect();
        for event in &events {
            match event_owner(event) {
                Some(SoundOwner::Mcr) => self.tally.mcr_owned += 1,
                Some(SoundOwner::Java) => self.tally.java_owned += 1,
                None => {}
            }
            if matches!(
                event,
                ClientEvent::Sound { .. } | ClientEvent::LocalSound { .. }
            ) {
                self.tally.server_sounds += 1;
            }
        }
        let player = self.client.state.player.physics.pos;
        while let Some(e) = self.bridge.try_event() {
            match e {
                BridgeEvent::Crash { report } => panic!("java crashed: {report:?}"),
                BridgeEvent::Info { key, value } if key == "sound_play" => {
                    let play = JavaPlay::parse(&value, player).expect("sound_play format");
                    self.tally.java_plays.push(play);
                }
                _ => {}
            }
        }
        let mut started = Vec::new();
        self.sound.take_started(&mut started);
        for v in started {
            if v.buffer.is_some() {
                self.tally.java_static.push(v);
            } else {
                self.tally.java_streams += 1;
            }
        }
        if let Some(rest) = Duration::from_millis(50).checked_sub(start.elapsed()) {
            std::thread::sleep(rest);
        }
    }

    fn idle(&mut self, ticks: u32) {
        for _ in 0..ticks {
            let input = self.client.idle_input();
            self.tick(&input);
        }
    }

    /// Runs `body`, lets Java catch up, and returns what both sides did meanwhile.
    fn phase(&mut self, name: &str, body: impl FnOnce(&mut Self)) -> Tally {
        self.tally = Tally::default();
        body(self);
        self.idle(SETTLE_TICKS);
        let t = std::mem::take(&mut self.tally);
        eprintln!(
            "{name}: server sounds {}, mcr-owned {}, java-owned {}, java plays {}, java static voices {}, streams {}",
            t.server_sounds,
            t.mcr_owned,
            t.java_owned,
            t.java_plays.len(),
            t.java_static.len(),
            t.java_streams
        );
        for p in &t.java_plays {
            eprintln!("  java {} at {:?}", p.id, p.pos);
        }
        t
    }
}

/// `(slot, generation)` of every playing or paused bridge voice.
fn live_slots(sound: &SoundLink) -> Vec<(usize, u32)> {
    (0..SOUND_SLOTS)
        .filter_map(|i| match sound.slot_state(i) {
            Some((generation, st)) if st == VOICE_PLAYING || st == VOICE_PAUSED => {
                Some((i, generation))
            }
            _ => None,
        })
        .collect()
}

fn spawn_capture(link: &SoundLink, stop: Arc<AtomicBool>) -> Arc<Mutex<usize>> {
    let mut mixer = Mixer::new(RATE);
    mixer.attach_producer(Box::new(link.voices(RATE).expect("first attach")));
    let active = Arc::new(Mutex::new(0));
    let a = active.clone();
    std::thread::spawn(move || {
        let start = Instant::now();
        let mut rendered = 0u64;
        let mut buf = vec![0.0f32; 1024];
        while !stop.load(Ordering::Acquire) {
            mixer.render(&mut buf);
            rendered += 512;
            *a.lock().unwrap() = mixer.active_voices();
            let due = Duration::from_secs_f64(rendered as f64 / f64::from(RATE));
            if let Some(wait) = due.checked_sub(start.elapsed()) {
                std::thread::sleep(wait);
            }
        }
    });
    active
}

#[test]
fn every_sound_plays_on_exactly_one_side() {
    if std::env::var("MCR_JVM_TESTS").as_deref() != Ok("1")
        || std::env::var("MCR_LIVE_ADDR").is_err()
    {
        eprintln!("skipped: set MCR_JVM_TESTS=1 and MCR_LIVE_ADDR (see the file header)");
        return;
    }
    let cache = mcr_jvm::dev_cache_dir();
    let manifest = LaunchManifest::load(&cache).expect("run `cargo xtask fabric-fetch`");
    let game_dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("sound-session");
    let _ = std::fs::remove_dir_all(&game_dir);
    let mut cfg = BridgeConfig::new(cache, game_dir, manifest.test_mods.clone());
    cfg.username = USERNAME.into();
    cfg.uuid = mcr_auth::offline::offline_uuid(USERNAME)
        .unwrap()
        .hyphenated()
        .to_string();
    cfg.max_heap = "1536m".into();
    cfg.extra_jvm = vec!["-Dmcr.sound.trace=true".into()];

    let sound = SoundLink::get().unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let active = spawn_capture(&sound, stop.clone());
    let mut bridge = Bridge::start(cfg).expect("bridge start");
    let start = Instant::now();
    loop {
        assert!(start.elapsed() < BOOT_DEADLINE, "no title screen");
        if let Some(e) = bridge.next_event(Duration::from_millis(100)) {
            if let BridgeEvent::Crash { report } = &e {
                panic!("java crashed: {report:?}");
            }
            if e.is_title_screen() {
                break;
            }
        }
    }

    let addr = env("MCR_LIVE_ADDR");
    let (host, port) = addr.rsplit_once(':').expect("host:port");
    let port: u16 = port.parse().unwrap();
    console(&format!("op {USERNAME}"));
    let mut ccfg = ClientConfig::default().with_addr(&addr);
    ccfg.name = USERNAME.into();
    ccfg.view_distance = 6;
    let hooks = bridge.net().hooks(host, port);
    let client = Client::connect_hooked(ccfg, hooks).expect("join through the bridge");
    let mut s = Session {
        bridge,
        client,
        sound: sound.clone(),
        tally: Tally::default(),
    };
    s.idle(100);
    console(&format!("gamemode creative {USERNAME}"));
    console("time set day");
    console("weather clear");
    console("gamerule spawn_mobs false");
    console("kill @e[type=!minecraft:player]");
    s.idle(40);

    let walk = s.phase("walk", |s| {
        for i in 0..60 {
            let mut input = s.client.idle_input();
            input.forward = true;
            input.jump = i % 20 == 0;
            s.tick(&input);
        }
    });
    let moved: Vec<_> = walk.java_plays.iter().filter(|p| p.is_movement()).collect();
    assert!(
        moved.is_empty(),
        "Java played the local player's movement: {moved:?}"
    );
    assert_eq!(walk.mcr_owned, 0);

    let [x, y, z] = s.client.state.player.physics.pos;
    let below = mcr_buf::BlockPos {
        x: x.floor() as i32,
        y: y.floor() as i32 - 1,
        z: z.floor() as i32,
    };
    let centre = [
        f64::from(below.x) + 0.5,
        f64::from(below.y) + 0.5,
        f64::from(below.z) + 0.5,
    ];
    let broken = s.phase("break", |s| {
        s.client.break_instantly(below, Facing::Up).expect("break");
    });
    assert_eq!(broken.mcr_owned, 1, "mcr predicts its own break");
    let doubled: Vec<_> = broken
        .java_plays
        .iter()
        .filter(|p| p.id.ends_with(".break") && p.near(centre, 0.5))
        .collect();
    assert!(doubled.is_empty(), "Java played mcr's break: {doubled:?}");

    let played = s.phase("playsound", |_| {
        console(&format!(
            "execute at {USERNAME} run playsound minecraft:entity.cow.ambient master {USERNAME}"
        ));
    });
    assert!(played.server_sounds >= 1, "the server sent no sound");
    assert_eq!(
        played.plays("minecraft:entity.cow.ambient"),
        1,
        "Java plays /playsound once"
    );
    assert!(!played.java_static.is_empty(), "no voice for /playsound");
    assert_eq!(played.mcr_owned, 0);

    let mobs = s.phase("mobs", |s| {
        console(&format!(
            "execute at {USERNAME} run summon minecraft:cow ~3 ~ ~"
        ));
        console(&format!(
            "execute at {USERNAME} run summon minecraft:chicken ~-3 ~ ~"
        ));
        s.idle(400);
    });
    let mob_plays = mobs.plays("minecraft:entity.cow") + mobs.plays("minecraft:entity.chicken");
    assert!(mob_plays > 0, "Java played no mob sounds");
    assert!(
        mob_plays <= mobs.server_sounds,
        "Java played mob sounds the server never sent"
    );
    assert_eq!(mobs.mcr_owned, 0);

    let mut before = Vec::new();
    let stopped = s.phase("stopsound", |s| {
        console(&format!(
            "execute at {USERNAME} run playsound minecraft:music_disc.cat record {USERNAME}"
        ));
        s.idle(20);
        before = live_slots(&s.sound);
        console(&format!("stopsound {USERNAME}"));
    });
    assert_eq!(stopped.plays("minecraft:music_disc.cat"), 1);
    assert!(stopped.java_streams >= 1, "no stream for the disc");
    assert!(!before.is_empty(), "nothing playing before /stopsound");
    // Java restarts ambience and music right after a stop, so only the voices live at the stop must be gone.
    let after = live_slots(&sound);
    let survived: Vec<_> = before.iter().filter(|v| after.contains(v)).collect();
    let stats = sound.stats();
    eprintln!(
        "stats {stats:?}, live before {}, after {}, mixer voices {}",
        before.len(),
        after.len(),
        *active.lock().unwrap()
    );
    assert!(
        survived.is_empty(),
        "voices survived /stopsound: {survived:?}"
    );
    let orphaned = (0..20).all(|_| {
        let mixed = *active.lock().unwrap();
        let live = live_slots(&sound).len();
        std::thread::sleep(Duration::from_millis(20));
        mixed > live
    });
    assert!(!orphaned, "mixer voices without a live slot");
    assert_eq!(stats.underruns, 0);
    assert_eq!(stats.dropped, 0);
    stop.store(true, Ordering::Release);
    let Session { bridge, .. } = s;
    let _ = bridge.shutdown(mcr_modbridge::SHUTDOWN_TIMEOUT);
}
