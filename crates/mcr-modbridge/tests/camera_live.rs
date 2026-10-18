//! Opt-in: while mcr's camera turns every frame in a joined world, Java renders from it and never hands a camera
//! back, so terrain shows the newest turn with no frame of Java's in between. Under either loader:
//! `MCR_JVM_TESTS=1 MCR_LIVE_ADDR=localhost:25577 [MCR_LIVE_LOADER=neoforge] [MCR_NEOFORGE_MODS=<dir>]
//!  cargo test -p mcr-modbridge --test camera_live -- --nocapture`

mod gpu_common;

use std::time::{Duration, Instant};

use gpu_common::compose::Rig;
use gpu_common::{config, neoforge_config};
use mcr_client::{Client, ClientConfig};
use mcr_modbridge::ShutdownOutcome;

const USERNAME: &str = "Turner";
const TITLE: &str = "net.minecraft.client.gui.screens.TitleScreen";
const TURN_FRAMES: u32 = 600;
const STEP: f32 = 0.6;

fn frame(rig: &mut Rig, client: &mut Client) {
    let ticks = rig.frame();
    for _ in 0..ticks {
        let input = client.idle_input();
        client.tick(&input).expect("client tick");
    }
}

#[test]
fn a_turning_camera_is_never_handed_back() {
    let (Ok("1"), Ok(addr)) = (
        std::env::var("MCR_JVM_TESTS").as_deref(),
        std::env::var("MCR_LIVE_ADDR"),
    ) else {
        eprintln!("skipped: set MCR_JVM_TESTS=1 and MCR_LIVE_ADDR (see the file header)");
        return;
    };
    let (host, port) = addr.rsplit_once(':').expect("host:port");
    let port: u16 = port.parse().expect("port");
    let neoforge = std::env::var("MCR_LIVE_LOADER").as_deref() == Ok("neoforge");
    let mut cfg = if neoforge {
        neoforge_config("camera-live", &[])
    } else {
        config("camera-live", &[])
    };
    cfg.http_hosts.clear();
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
    let start = Instant::now();
    while !(rig.screen.is_empty() && rig.frames.lock().java().loaded) {
        assert!(
            start.elapsed() < Duration::from_secs(120),
            "java never left the loading screen"
        );
        frame(&mut rig, &mut client);
    }
    let settle = Instant::now();
    while settle.elapsed() < Duration::from_secs(3) {
        frame(&mut rig, &mut client);
    }
    assert!(
        rig.frames.lock().world_active(),
        "java draws the world before the camera turns"
    );

    let mut handed_back = 0;
    for i in 0..TURN_FRAMES {
        rig.camera.yaw += STEP.to_radians();
        rig.camera.pitch = (i as f32 * 0.05).sin() * 0.6;
        frame(&mut rig, &mut client);
        if let Some(camera) = rig.frames.lock().java_camera() {
            if handed_back == 0 {
                eprintln!("frame {i}: java handed back {camera:?}");
            }
            handed_back += 1;
        }
    }
    eprintln!(
        "{} loader: {handed_back} of {TURN_FRAMES} turning frames handed the camera back",
        if neoforge { "neoforge" } else { "fabric" }
    );
    assert_eq!(handed_back, 0, "java rendered from its own camera");

    drop(client);
    assert_eq!(rig.detached, None, "the bridge detached");
    let outcome = rig.bridge.shutdown(mcr_modbridge::SHUTDOWN_TIMEOUT);
    assert!(
        matches!(outcome, ShutdownOutcome::Clean { .. }),
        "{outcome:?}"
    );
}
