//! Opt-in: Java draws entities, block entities, nametags, outlines and a mod's render layer into mcr's
//! world against a vanilla 26.3 server (`cargo xtask server`), checked with pixel probes.
//! `MCR_JVM_TESTS=1 MCR_LIVE_ADDR=localhost:25568 MCR_LIVE_CONSOLE=<file the server's stdin tails>
//!  cargo test -p mcr-app --test entity_scene -- --nocapture --test-threads=1`
//! `MCR_ENTITY_LOAD=1` adds the Java frame-time run with 0, 100 and 500 entities.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const NAME: &str = "Scener";
const WIDTH: u32 = 1920;
const HEIGHT: u32 = 1080;
const EYE: [f64; 3] = [0.5, 151.0 + 1.62, 0.5];
const PITCH_DEG: f64 = 12.0;
const FOV_DEG: f64 = 70.0;
const RUN_LIMIT: Duration = Duration::from_secs(300);

const STAGE: &[&str] = &[
    "forceload add -16 -16 16 32",
    "gamerule advance_time false",
    "time set noon",
    "weather clear 100000",
    "kill @e[type=!player]",
    "fill -8 150 -2 8 150 20 stone",
    "fill -8 151 -2 8 158 20 air",
    "fill -4 151 10 4 156 10 stone",
    "setblock 6 151 8 stone",
];

const HELMET: &str = r#"equipment:{head:{id:"minecraft:leather_helmet",count:1,components:{"minecraft:unbreakable":{}}}}"#;

fn scene() -> Vec<String> {
    vec![
        "kill @e[type=item]".into(),
        format!(
            r#"summon zombie -1.2 151 6.5 {{NoAI:1b,Silent:1b,PersistenceRequired:1b,Rotation:[180f,0f],CustomName:"Zed",CustomNameVisible:1b,{HELMET}}}"#
        ),
        "summon armor_stand 1.2 151 6.5 {Rotation:[180f,0f]}".into(),
        "summon pig -4 151 6.5 {NoAI:1b,Silent:1b,Glowing:1b,Rotation:[90f,0f]}".into(),
        "setblock 3 151 6 chest[facing=north]".into(),
        r#"setblock -6 151 6 oak_sign[rotation=8]{front_text:{messages:["Hi","mcr","",""]}}"#
            .into(),
        r#"summon item_frame 6.5 151.5 7.5 {Facing:2b,Item:{id:"minecraft:diamond",count:1}}"#
            .into(),
        format!(
            "summon zombie 0.5 151 12.5 {{NoAI:1b,Silent:1b,PersistenceRequired:1b,Rotation:[180f,0f],{HELMET}}}"
        ),
    ]
}

struct Live {
    addr: String,
    console: PathBuf,
}

fn live() -> Option<Live> {
    let (Ok("1"), Ok(addr), Ok(console)) = (
        std::env::var("MCR_JVM_TESTS").as_deref(),
        std::env::var("MCR_LIVE_ADDR"),
        std::env::var("MCR_LIVE_CONSOLE"),
    ) else {
        eprintln!(
            "skipped: set MCR_JVM_TESTS=1, MCR_LIVE_ADDR and MCR_LIVE_CONSOLE (see the file header)"
        );
        return None;
    };
    Some(Live {
        addr,
        console: console.into(),
    })
}

impl Live {
    fn send(&self, lines: &[String]) {
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&self.console)
            .unwrap();
        for line in lines {
            writeln!(f, "{line}").unwrap();
        }
    }
}

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A fresh game dir with the test mods and Java options that skip the tutorial toasts.
fn prepare(tag: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("entity-scene-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    let mods = dir.join("mods");
    std::fs::create_dir_all(&mods).unwrap();
    std::fs::create_dir_all(dir.join("game/java")).unwrap();
    let cache = repo().join(".cache");
    let fabric_api = std::fs::read_dir(cache.join("mods-test"))
        .expect("run `cargo xtask fabric-fetch`")
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("fabric-api-"))
        })
        .expect("the pinned Fabric API jar");
    std::fs::copy(&fabric_api, mods.join(fabric_api.file_name().unwrap())).unwrap();
    std::fs::copy(
        cache.join("java/mcr-testmod.jar"),
        mods.join("mcr-testmod.jar"),
    )
    .expect("run `cargo xtask java-build`");
    std::fs::write(
        dir.join("game/java/options.txt"),
        "onboardAccessibility:false\ntutorialStep:none\njoinedFirstServer:true\n",
    )
    .unwrap();
    dir
}

/// Runs `mcr` offscreen with `script`; `on_line` sees each stdout line and returns console commands.
fn run_client(
    live: &Live,
    dir: &Path,
    script: &str,
    mut on_line: impl FnMut(&str) -> Vec<String>,
) -> Vec<String> {
    let script_path = dir.join("script.txt");
    std::fs::write(&script_path, script).unwrap();
    let mut child: Child = Command::new(env!("CARGO_BIN_EXE_mcr"))
        .current_dir(repo())
        .args(["--connect", &live.addr, "--name", NAME, "--size"])
        .arg(format!("{WIDTH}x{HEIGHT}"))
        .arg("--mods")
        .arg(dir.join("mods"))
        .arg("--game-dir")
        .arg(dir.join("game"))
        .arg("--autopilot")
        .arg(&script_path)
        .arg("--screenshot")
        .arg(dir.join("final.png"))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mcr");
    let (tx, rx) = mpsc::channel();
    let out: Box<dyn std::io::Read + Send> = Box::new(child.stdout.take().unwrap());
    let err: Box<dyn std::io::Read + Send> = Box::new(child.stderr.take().unwrap());
    for stream in [out, err] {
        let tx = tx.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stream).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
    }
    drop(tx);
    let start = Instant::now();
    let mut lines = Vec::new();
    loop {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(line) => {
                if !line.starts_with('[') || line.starts_with("[mods]") {
                    eprintln!("mcr: {line}");
                }
                let commands = on_line(&line);
                live.send(&commands);
                lines.push(line);
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if start.elapsed() > RUN_LIMIT {
            let _ = child.kill();
            panic!("mcr did not finish within {RUN_LIMIT:?}");
        }
    }
    let status = child.wait().unwrap();
    assert!(status.success(), "mcr exited with {status}");
    lines
}

fn join_commands() -> Vec<String> {
    vec![
        format!("gamemode creative {NAME}"),
        format!("tp {NAME} 0.5 151 0.5 0 {PITCH_DEG}"),
    ]
}

struct Image {
    pixels: Vec<u8>,
}

impl Image {
    fn load(path: &Path) -> Self {
        let decoder =
            png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).unwrap()));
        let mut reader = decoder.read_info().unwrap();
        let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut pixels).unwrap();
        assert_eq!((info.width, info.height), (WIDTH, HEIGHT));
        assert_eq!(info.color_type, png::ColorType::Rgba);
        Self { pixels }
    }

    fn rgb(&self, x: u32, y: u32) -> [u8; 3] {
        let i = ((y * WIDTH + x) * 4) as usize;
        [self.pixels[i], self.pixels[i + 1], self.pixels[i + 2]]
    }
}

/// Screen position of a world point for the scripted camera (vanilla yaw 0 looks toward +Z).
fn project(p: [f64; 3]) -> (f64, f64) {
    let pitch = PITCH_DEG.to_radians();
    let d = [p[0] - EYE[0], p[1] - EYE[1], p[2] - EYE[2]];
    let forward = d[2] * pitch.cos() - d[1] * pitch.sin();
    let right = -d[0];
    let up = d[1] * pitch.cos() + d[2] * pitch.sin();
    let t = (FOV_DEG.to_radians() / 2.0).tan();
    let aspect = f64::from(WIDTH) / f64::from(HEIGHT);
    let x = (right / forward / (t * aspect) + 1.0) / 2.0 * f64::from(WIDTH);
    let y = (1.0 - up / forward / t) / 2.0 * f64::from(HEIGHT);
    (x, y)
}

#[derive(Debug, Clone, Copy)]
struct Probe {
    name: &'static str,
    at: [f64; 3],
    /// Half the box size in blocks.
    half: f64,
}

fn region(probe: Probe) -> impl Iterator<Item = (u32, u32)> {
    let (cx, cy) = project(probe.at);
    let depth = (probe.at[2] - EYE[2]).max(1.0);
    let px = probe.half * f64::from(HEIGHT) / (2.0 * (FOV_DEG.to_radians() / 2.0).tan() * depth);
    let x0 = (cx - px).max(0.0) as u32;
    let x1 = ((cx + px) as u32).min(WIDTH - 1);
    let y0 = (cy - px).max(0.0) as u32;
    let y1 = ((cy + px) as u32).min(HEIGHT - 1);
    (y0..=y1).flat_map(move |y| (x0..=x1).map(move |x| (x, y)))
}

fn changed(base: &Image, scene: &Image, probe: Probe) -> (u32, u32) {
    let mut n = 0;
    let mut total = 0;
    for (x, y) in region(probe) {
        total += 1;
        let (a, b) = (base.rgb(x, y), scene.rgb(x, y));
        if (0..3).any(|c| a[c].abs_diff(b[c]) > 40) {
            n += 1;
        }
    }
    (n, total)
}

fn count(img: &Image, probe: Probe, pred: impl Fn([u8; 3]) -> bool) -> u32 {
    region(probe).filter(|&(x, y)| pred(img.rgb(x, y))).count() as u32
}

#[test]
fn java_draws_entities_into_mcrs_world() {
    let Some(live) = live() else {
        return;
    };
    let dir = prepare("probes");
    live.send(&STAGE.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>());
    live.send(&[
        "setblock 3 151 6 air".into(),
        "setblock -6 151 6 air".into(),
    ]);
    let base = dir.join("base.png");
    let shot = dir.join("scene.png");
    let script = format!(
        "wait 200\nshot {}\nwait 200\nshot {}\nwait 5\n",
        base.display(),
        shot.display()
    );
    let mut summoned = false;
    let lines = run_client(&live, &dir, &script, |line| {
        if line.starts_with("ready:") {
            join_commands()
        } else if line.starts_with("wrote") && line.contains("base.png") && !summoned {
            summoned = true;
            scene()
        } else {
            Vec::new()
        }
    });
    assert!(
        lines.iter().any(|l| l.contains("shutdown: Clean")),
        "Java did not shut down cleanly"
    );
    let stats = lines
        .iter()
        .rev()
        .find_map(|l| {
            l.split("value: \"")
                .nth(1)
                .filter(|_| l.contains("world_render"))
        })
        .expect("a world_render report");
    let field = |key: &str| -> u32 {
        stats
            .split_whitespace()
            .find_map(|kv| {
                kv.strip_prefix(key)?
                    .strip_prefix('=')?
                    .trim_end_matches(['"', '}', ' '])
                    .parse()
                    .ok()
            })
            .unwrap_or(0)
    };
    eprintln!("world_render: {stats}");
    assert!(
        field("entities") >= 5,
        "Java extracted too few entities: {stats}"
    );
    assert!(
        field("block_entities") >= 2,
        "the chest and sign are not in Java's visible sections: {stats}"
    );
    assert!(field("scans") > 0, "no meshless section scans: {stats}");

    let base = Image::load(&base);
    let scene_img = Image::load(&shot);

    let visible = [
        Probe {
            name: "zombie",
            at: [-1.2, 152.0, 6.5],
            half: 0.3,
        },
        Probe {
            name: "armor stand",
            at: [1.2, 151.8, 6.5],
            half: 0.25,
        },
        Probe {
            name: "pig",
            at: [-4.0, 151.5, 6.5],
            half: 0.35,
        },
        Probe {
            name: "chest",
            at: [3.5, 151.4, 6.5],
            half: 0.35,
        },
        Probe {
            name: "sign",
            at: [-5.5, 152.2, 6.5],
            half: 0.25,
        },
        Probe {
            name: "item frame",
            at: [6.5, 151.5, 7.9],
            half: 0.25,
        },
    ];
    for probe in visible {
        let (n, total) = changed(&base, &scene_img, probe);
        eprintln!("{}: {n}/{total} pixels changed", probe.name);
        assert!(n * 4 > total, "{} not drawn ({n}/{total})", probe.name);
    }
    let hidden = Probe {
        name: "occluded zombie",
        at: [0.5, 152.0, 12.5],
        half: 0.3,
    };
    let (n, total) = changed(&base, &scene_img, hidden);
    eprintln!("occluded zombie: {n}/{total} pixels changed");
    assert!(
        n * 50 < total,
        "the zombie behind the wall shows through ({n}/{total})"
    );

    let nametag = Probe {
        name: "nametag",
        at: [-1.2, 153.4, 6.5],
        half: 0.3,
    };
    let white = |[r, g, b]: [u8; 3]| r > 200 && g > 200 && b > 200;
    let (before, after) = (
        count(&base, nametag, white),
        count(&scene_img, nametag, white),
    );
    eprintln!("nametag: {before} -> {after} white pixels");
    assert!(after > before + 100, "the zombie's nametag is missing");

    let marker = Probe {
        name: "marker",
        at: [1.2, 153.25, 6.5],
        half: 0.3,
    };
    let magenta = |[r, g, b]: [u8; 3]| r > 140 && b > 140 && g < 90;
    let (before, after) = (
        count(&base, marker, magenta),
        count(&scene_img, marker, magenta),
    );
    eprintln!("marker layer: {before} -> {after} magenta pixels");
    assert!(
        before < 10 && after > 200,
        "the test mod's armor stand layer is missing"
    );

    let outline = Probe {
        name: "pig outline",
        at: [-4.0, 151.5, 6.5],
        half: 0.6,
    };
    let (before, after) = (
        count(&base, outline, white),
        count(&scene_img, outline, white),
    );
    eprintln!("glowing pig outline: {before} -> {after} white pixels");
    assert!(after > before + 100, "the glowing pig has no outline");
}

fn report_fov_scales(lines: &[String]) -> Vec<f32> {
    lines
        .iter()
        .filter(|l| l.starts_with("report:"))
        .filter_map(|l| l.rsplit("fov_scale ").next()?.trim().parse().ok())
        .collect()
}

fn reddish(c: [u8; 3]) -> bool {
    c[0] > 150 && c[1] < 90 && c[2] < 90
}

/// The test key starts a burst: the test mod's own particle type in front of the camera and a halved
/// field of view, which mcr must apply to its whole world view.
#[test]
fn mod_particles_and_zoom_reach_mcrs_world() {
    let Some(live) = live() else {
        return;
    };
    let dir = prepare("burst");
    live.send(&STAGE.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>());
    let base = dir.join("base.png");
    let burst = dir.join("burst.png");
    let script = format!(
        "wait 200\nshot {}\nreport\ntap k\nwait 20\nreport\nshot {}\nwait 140\nreport\n",
        base.display(),
        burst.display()
    );
    let lines = run_client(&live, &dir, &script, |line| {
        if line.starts_with("ready:") {
            join_commands()
        } else {
            Vec::new()
        }
    });
    let scales = report_fov_scales(&lines);
    assert_eq!(scales.len(), 3, "three reports: {lines:?}");
    assert!(
        (scales[0] - 1.0).abs() < 0.01,
        "no zoom before the key: {scales:?}"
    );
    assert!(
        (scales[1] - 0.5).abs() < 0.01,
        "the mod's zoom did not reach mcr: {scales:?}"
    );
    assert!(
        (scales[2] - 1.0).abs() < 0.01,
        "the zoom did not end: {scales:?}"
    );
    let particles = lines
        .iter()
        .filter(|l| l.contains("world_render"))
        .filter_map(|l| {
            l.split("particles=")
                .nth(1)?
                .split(['"', ' '])
                .next()?
                .parse::<u32>()
                .ok()
        })
        .max()
        .unwrap_or(0);
    assert!(
        particles > 0,
        "Java spawned none of the test mod's particles"
    );
    let centre = Probe {
        name: "burst",
        at: [EYE[0], EYE[1] - 0.6, EYE[2] + 2.5],
        half: 0.6,
    };
    let before = count(&Image::load(&base), centre, reddish);
    let during = count(&Image::load(&burst), centre, reddish);
    eprintln!("{}: red pixels {before} -> {during}", centre.name);
    assert!(
        during > before + 200,
        "the test mod's particles are not visible: {before} -> {during}"
    );
}

/// Frame times with many entities in view, for the design notes.
#[test]
fn java_frame_time_with_entity_counts() {
    let Some(live) = live() else {
        return;
    };
    if std::env::var("MCR_ENTITY_LOAD").as_deref() != Ok("1") {
        eprintln!("skipped: set MCR_ENTITY_LOAD=1");
        return;
    }
    for n in [0usize, 100, 500] {
        let dir = prepare(&format!("load-{n}"));
        live.send(&STAGE.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>());
        let mut spawn = Vec::new();
        for i in 0..n {
            let (x, z) = ((i % 25) as f64 * 0.6 - 7.0, 4.0 + (i / 25) as f64 * 0.8);
            spawn.push(format!(
                "summon zombie {x:.1} 151 {z:.1} {{NoAI:1b,Silent:1b,PersistenceRequired:1b,Rotation:[180f,0f],{HELMET}}}"
            ));
        }
        live.send(&spawn);
        let lines = run_client(&live, &dir, "wait 100\nwait 600\n", |line| {
            if line.starts_with("ready:") {
                join_commands()
            } else {
                Vec::new()
            }
        });
        for line in lines
            .iter()
            .filter(|l| l.contains("composed") || l.contains("frame time"))
        {
            println!("{n} entities: {line}");
        }
    }
    live.send(&["kill @e[type=!player]".into()]);
}
