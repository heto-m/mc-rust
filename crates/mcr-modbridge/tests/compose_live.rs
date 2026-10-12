//! Opt-in: Java's HUD composed over an offscreen mcr frame while joined to a vanilla 26.3 server.
//! `MCR_JVM_TESTS=1 MCR_LIVE_ADDR=localhost:25567 MCR_LIVE_CONSOLE=<file the server's stdin tails>
//!  cargo test -p mcr-modbridge --test compose_live -- --nocapture`

mod gpu_common;

use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

use gpu_common::compose::*;
use gpu_common::config;
use gpu_common::image::save_png;
use mcr_client::{Client, ClientConfig};
use mcr_modbridge::{ComposeStats, ShutdownOutcome};

const USERNAME: &str = "Composer";
const TITLE: &str = "net.minecraft.client.gui.screens.TitleScreen";
const SESSION: Duration = Duration::from_secs(60);

fn console(line: &str) {
    let path = std::env::var("MCR_LIVE_CONSOLE").expect("set MCR_LIVE_CONSOLE");
    let mut f = std::fs::OpenOptions::new().append(true).open(path).unwrap();
    writeln!(f, "{line}").unwrap();
}

/// One frame of the joined session: Rust's client runs exactly the ticks Java is given.
fn frame(rig: &mut Rig, client: &mut Client) {
    let ticks = rig.frame();
    for _ in 0..ticks {
        let input = client.idle_input();
        client.tick(&input).expect("client tick");
    }
}

fn run(rig: &mut Rig, client: &mut Client, d: Duration) {
    let start = Instant::now();
    while start.elapsed() < d {
        frame(rig, client);
    }
}

fn differs(a: &[u8], b: &[u8]) -> bool {
    (0..3).any(|c| a[c].abs_diff(b[c]) > 24)
}

/// Pixels of `r` that differ from the same row's leftmost pixel (bare scene there).
fn against_row_edge(img: &Rgba, r: Widget) -> u32 {
    let mut n = 0;
    for y in r.y..(r.y + r.h).min(img.height) {
        let edge = ((y * img.width + 2) * 4) as usize;
        for x in r.x..(r.x + r.w).min(img.width) {
            let i = ((y * img.width + x) * 4) as usize;
            if differs(&img.pixels[i..i + 3], &img.pixels[edge..edge + 3]) {
                n += 1;
            }
        }
    }
    n
}

fn red_pixels(img: &Rgba, r: Widget) -> u32 {
    let mut n = 0;
    for y in r.y..(r.y + r.h).min(img.height) {
        for x in r.x..(r.x + r.w).min(img.width) {
            let i = ((y * img.width + x) * 4) as usize;
            let (red, g, b) = (img.pixels[i], img.pixels[i + 1], img.pixels[i + 2]);
            if red > 120 && u16::from(red) > 2 * u16::from(g) && u16::from(red) > 2 * u16::from(b) {
                n += 1;
            }
        }
    }
    n
}

/// Hotbar, hearts and crosshair at vanilla's layout for the current size.
fn assert_hud(rig: &Rig, tag: &str) {
    let img = rig.pixels();
    let (w, h) = (img.width, img.height);
    let s = rig.gui_scale();
    save_png(
        &Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("compose-live-{tag}.png")),
        &img,
    );
    let hotbar = Widget {
        x: w / 2 - 91 * s,
        y: h - 22 * s,
        w: 182 * s,
        h: 22 * s,
    };
    let hearts = Widget {
        x: w / 2 - 91 * s,
        y: h - 39 * s,
        w: 81 * s,
        h: 9 * s,
    };
    let cross = Widget {
        x: w / 2 - 7 * s,
        y: h / 2 - 7 * s,
        w: 15 * s,
        h: 15 * s,
    };
    let area = |r: Widget| r.w * r.h;
    let (hb, hr, cr) = (
        against_row_edge(&img, hotbar),
        red_pixels(&img, hearts),
        against_row_edge(&img, cross),
    );
    eprintln!(
        "{tag}: {w}x{h} gui {s}: hotbar {hb}/{} hearts {hr} crosshair {cr}",
        area(hotbar)
    );
    assert!(hb > area(hotbar) / 3, "{tag}: no hotbar ({hb} px)");
    assert!(hr > 20 * s * s, "{tag}: no hearts ({hr} px)");
    assert!(cr > 8 * s, "{tag}: no crosshair ({cr} px)");
}

#[test]
fn hud_resize_and_ticks_against_a_live_server() {
    let (Ok("1"), Ok(addr)) = (
        std::env::var("MCR_JVM_TESTS").as_deref(),
        std::env::var("MCR_LIVE_ADDR"),
    ) else {
        eprintln!("skipped: set MCR_JVM_TESTS=1 and MCR_LIVE_ADDR (see the file header)");
        return;
    };
    let (host, port) = addr.rsplit_once(':').expect("host:port");
    let port: u16 = port.parse().expect("port");
    let mut cfg = config("compose-live", &[]);
    (cfg.width, cfg.height) = (1920, 1080);
    cfg.username = USERNAME.into();
    cfg.uuid = mcr_auth::offline::offline_uuid(USERNAME)
        .unwrap()
        .hyphenated()
        .to_string();
    let Some(mut rig) = Rig::start(cfg) else {
        return;
    };
    rig.until("the title screen", |r| r.screen == TITLE && r.loaded());

    let link = rig.bridge.net();
    let mut ccfg = ClientConfig::default().with_addr(&format!("{host}:{port}"));
    ccfg.name = USERNAME.into();
    let mut client = Client::connect_hooked(ccfg, link.hooks(host, port)).expect("join");
    let joined = Instant::now();
    console(&format!("gamemode survival {USERNAME}"));
    let start = Instant::now();
    while !(rig.screen.is_empty() && rig.frames.lock().java().loaded) {
        assert!(
            start.elapsed() < Duration::from_secs(120),
            "java never left the loading screen"
        );
        frame(&mut rig, &mut client);
    }
    run(&mut rig, &mut client, Duration::from_secs(3));
    assert_hud(&rig, "1920x1080");

    console("say compose-check");
    run(&mut rig, &mut client, Duration::from_secs(1));
    let img = rig.pixels();
    let s = rig.gui_scale();
    let chat = Widget {
        x: 2 * s,
        y: img.height - 52 * s,
        w: 150 * s,
        h: 16 * s,
    };
    let (chat_px, _) = text_pixels(&img, chat);
    save_png(
        &Path::new(env!("CARGO_TARGET_TMPDIR")).join("compose-live-chat.png"),
        &img,
    );
    assert!(
        chat_px > 20 * s * s,
        "no chat line ({chat_px} px in {chat:?})"
    );

    for (w, h) in [(1600, 900), (1280, 720), (1920, 1080)] {
        rig.renderer.resize(w, h);
        run(&mut rig, &mut client, Duration::from_millis(1500));
        assert_eq!(
            rig.frames.lock().java().size,
            (w, h),
            "java main target follows"
        );
        assert_hud(&rig, &format!("{w}x{h}"));
    }

    rig.frames.lock().reset_stats();
    let left = SESSION.saturating_sub(joined.elapsed());
    run(&mut rig, &mut client, left.max(Duration::from_secs(20)));
    let stats = rig.stats();
    let (w50, w99, wmax) = ComposeStats::percentiles(&stats.waits_us);
    let (j50, j99, jmax) = ComposeStats::percentiles(&stats.java_us);
    eprintln!(
        "1920x1080 in game, {} frames: render-thread wait p50 {w50} us p99 {w99} us max {wmax} us; java frame p50 {j50} us p99 {j99} us max {jmax} us; late {}",
        stats.composed, stats.late
    );

    rig.stop_ticks();
    run(&mut rig, &mut client, Duration::from_secs(1));
    let stats = rig.stats();
    eprintln!(
        "after {:.0}s joined: java ticks {} rust ticks {}",
        joined.elapsed().as_secs_f32(),
        stats.java_ticks,
        stats.rust_ticks
    );
    assert!(joined.elapsed() >= SESSION);
    assert_eq!(stats.java_ticks, stats.rust_ticks, "tick counters agree");

    drop(client);
    let backend = mcr_modbridge::gpu::backend().expect("backend").stats();
    eprintln!("backend stats: {backend:?}");
    assert_eq!(rig.detached, None, "the bridge detached");
    let outcome = rig.bridge.shutdown(mcr_modbridge::SHUTDOWN_TIMEOUT);
    assert!(
        matches!(outcome, ShutdownOutcome::Clean { .. }),
        "{outcome:?}"
    );
}
