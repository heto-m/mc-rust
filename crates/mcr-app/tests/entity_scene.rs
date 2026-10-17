//! Opt-in: Java draws entities, block entities, nametags, outlines and a mod's render layer into mcr's
//! world against a vanilla 26.3 server (`cargo xtask server`), checked with pixel probes.
//! `MCR_JVM_TESTS=1 MCR_LIVE_ADDR=localhost:25568 MCR_LIVE_CONSOLE=<file the server's stdin tails>
//!  cargo test -p mcr-app --test entity_scene -- --nocapture --test-threads=1`
//! `MCR_ENTITY_LOAD=1` adds the Java frame-time run with 0, 100 and 500 entities, and
//! `MCR_SECTION_LOAD=1` the one with 500 entities sealed in stone, with and without mcr's sections.

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
    on_line: impl FnMut(&str) -> Vec<String>,
) -> Vec<String> {
    run_client_with(live, dir, script, &[], on_line)
}

/// [`run_client`] with Java system properties for the embedded JVM.
fn run_client_with(
    live: &Live,
    dir: &Path,
    script: &str,
    java_props: &[&str],
    mut on_line: impl FnMut(&str) -> Vec<String>,
) -> Vec<String> {
    let script_path = dir.join("script.txt");
    std::fs::write(&script_path, script).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_mcr"));
    if !java_props.is_empty() {
        cmd.env("JAVA_TOOL_OPTIONS", java_props.join(" "));
    }
    let mut child: Child = cmd
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
        .filter_map(|l| {
            l.split("fov_scale ")
                .nth(1)?
                .split_whitespace()
                .next()?
                .parse()
                .ok()
        })
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

#[derive(Debug, Clone, PartialEq)]
struct Report {
    pos: [f64; 3],
    camera: [f64; 3],
    java_camera: String,
    java_roll: f64,
    java_weather: bool,
    java_steps: bool,
    java_turn: bool,
    mod_input: String,
    perspective: String,
    view_roll: f64,
}

fn reports(lines: &[String]) -> Vec<Report> {
    let floats = |l: &str, key: &str| -> Option<[f64; 3]> {
        let mut it = l.split(key).nth(1)?.split_whitespace();
        let mut v = [0.0; 3];
        for c in &mut v {
            *c = it.next()?.parse().ok()?;
        }
        Some(v)
    };
    let word = |l: &str, key: &str| -> Option<String> {
        Some(l.split(key).nth(1)?.split_whitespace().next()?.to_owned())
    };
    lines
        .iter()
        .filter(|l| l.starts_with("report:"))
        .filter_map(|l| {
            Some(Report {
                pos: floats(l, " pos ")?,
                camera: floats(l, " camera ")?,
                java_camera: word(l, " java_camera ")?,
                java_roll: word(l, " java_roll ")?.parse().ok()?,
                java_weather: word(l, " java_weather ")? == "true",
                java_steps: word(l, " java_steps ")? == "true",
                java_turn: word(l, " java_turn ")? == "true",
                mod_input: word(l, " mod input ")?,
                perspective: word(l, " perspective ")?,
                view_roll: word(l, " view_roll ")?.parse().ok()?,
            })
        })
        .collect()
}

/// The test mod's camera key: a camera lifted three blocks, then a free camera flown with the
/// movement keys while the player stands still, then mcr's own camera again.
#[test]
fn mod_cameras_hand_off_to_mcr() {
    let Some(live) = live() else {
        return;
    };
    let dir = prepare("camera");
    live.send(&STAGE.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>());
    let (base, nudged, flown) = (
        dir.join("base.png"),
        dir.join("nudged.png"),
        dir.join("flown.png"),
    );
    let script = format!(
        "wait 200\nreport\nshot {}\ntap j\nwait 20\nreport\nshot {}\ntap j\nwait 10\nreport\nhold w 8\nwait 5\nreport\nshot {}\ntap j\nwait 20\nreport\n",
        base.display(),
        nudged.display(),
        flown.display()
    );
    let lines = run_client(&live, &dir, &script, |line| {
        if line.starts_with("ready:") {
            join_commands().into_iter().chain(scene()).collect()
        } else {
            Vec::new()
        }
    });
    let r = reports(&lines);
    assert_eq!(r.len(), 5, "five reports: {r:?}");
    let eye = |r: &Report| [r.pos[0], r.pos[1] + 1.62, r.pos[2]];
    let near = |a: [f64; 3], b: [f64; 3], tol: f64| (0..3).all(|i| (a[i] - b[i]).abs() < tol);
    eprintln!("{r:#?}");
    assert_eq!(r[0].java_camera, "none");
    assert!(
        near(r[0].camera, eye(&r[0]), 0.01),
        "mcr's own camera: {r:?}"
    );
    assert_eq!(r[1].java_camera, "moved", "the lifted camera reached mcr");
    assert!(
        near(
            r[1].camera,
            [r[1].pos[0], r[1].pos[1] + 1.62 + 3.0, r[1].pos[2]],
            0.05
        ),
        "mcr renders from three blocks up: {r:?}"
    );
    assert_eq!(
        r[2].java_camera, "captured",
        "the free camera took the input"
    );
    assert!(
        r[3].camera[2] > r[2].camera[2] + 3.0,
        "the movement keys flew the free camera: {r:?}"
    );
    assert!(
        near(r[3].pos, r[2].pos, 0.01),
        "the player stood still: {r:?}"
    );
    assert_eq!(r[4].java_camera, "none");
    assert!(
        near(r[4].camera, eye(&r[4]), 0.01),
        "mcr's camera is back: {r:?}"
    );

    let (base, nudged) = (Image::load(&base), Image::load(&nudged));
    let moved = (0..HEIGHT)
        .step_by(4)
        .flat_map(|y| (0..WIDTH).step_by(4).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let (a, b) = (base.rgb(x, y), nudged.rgb(x, y));
            (0..3).any(|c| a[c].abs_diff(b[c]) > 40)
        })
        .count();
    eprintln!("lifted camera: {moved} sampled pixels changed");
    assert!(moved > 5000, "the terrain did not follow the lifted camera");
}

/// A mod that redirects the third-person pull-back (like Shoulder Surfing) moves mcr's camera; without
/// one, Java's third-person camera equals mcr's and nothing is handed off.
#[test]
fn third_person_offsets_reach_mcr() {
    let Some(live) = live() else {
        return;
    };
    let script = "wait 200\ntap f5\nwait 20\nreport\ntap f5\ntap f5\nwait 5\n";
    let mut cameras = Vec::new();
    for (props, shoulder) in [
        (&["-Dmcr.testmod.shoulder=true"][..], true),
        (&[][..], false),
    ] {
        let dir = prepare(&format!("shoulder-{shoulder}"));
        live.send(&STAGE.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>());
        let lines = run_client_with(&live, &dir, script, props, |line| {
            if line.starts_with("ready:") {
                join_commands()
            } else {
                Vec::new()
            }
        });
        let r = reports(&lines);
        assert_eq!(r.len(), 1, "{r:?}");
        let want = if shoulder { "moved" } else { "none" };
        assert_eq!(r[0].java_camera, want, "{props:?}: {r:?}");
        cameras.push(r[0].camera);
    }
    let offset = (0..3)
        .map(|a| (cameras[0][a] - cameras[1][a]).powi(2))
        .sum::<f64>()
        .sqrt();
    assert!(
        (offset - 1.0).abs() < 0.1,
        "the shoulder camera sits a block aside: {cameras:?}"
    );
}

/// A mod that rolls Java's camera about its view axis rolls mcr's view too.
#[test]
fn rolled_mod_cameras_roll_mcrs_view() {
    let Some(live) = live() else {
        return;
    };
    let dir = prepare("camera-roll");
    live.send(&STAGE.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>());
    let shot = dir.join("rolled.png");
    let script = format!("wait 200\nreport\nshot {}\nwait 5\n", shot.display());
    let lines = run_client_with(
        &live,
        &dir,
        &script,
        &["-Dmcr.testmod.cameraRoll=true"],
        |line| {
            if line.starts_with("ready:") {
                join_commands()
            } else {
                Vec::new()
            }
        },
    );
    let r = reports(&lines);
    assert_eq!(r.len(), 1, "{r:?}");
    assert_eq!(r[0].java_camera, "moved", "{r:?}");
    assert!(
        (r[0].java_roll + 15.0).abs() < 0.5,
        "the view rolls opposite to the camera: {r:?}"
    );
}

/// Java's camera type leads mcr's perspective: F5 cycles it in Java, and a mod that takes F5 for its
/// own cycle (like Shoulder Surfing) decides where mcr's camera goes.
#[test]
fn java_perspective_leads_mcrs_camera() {
    let Some(live) = live() else {
        return;
    };
    let script = "wait 200\nreport\ntap f5\nwait 20\nreport\ntap f5\nwait 20\nreport\ntap f5\nwait 20\nreport\n";
    for (props, cycle) in [
        (
            &["-Dmcr.testmod.perspective=true"][..],
            ["first", "front", "back", "first"],
        ),
        (&[][..], ["first", "back", "front", "first"]),
    ] {
        let dir = prepare(&format!("perspective-{}", props.len()));
        live.send(&STAGE.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>());
        let lines = run_client_with(&live, &dir, script, props, |line| {
            if line.starts_with("ready:") {
                join_commands()
            } else {
                Vec::new()
            }
        });
        let r = reports(&lines);
        eprintln!("{props:?}: {r:#?}");
        assert_eq!(r.len(), 4, "{r:?}");
        let seen: Vec<&str> = r.iter().map(|r| r.perspective.as_str()).collect();
        assert_eq!(seen, cycle, "{props:?}");
        for r in &r {
            assert_eq!(r.java_camera, "none", "settled without a handoff: {r:?}");
            let eye_z = r.pos[2];
            match r.perspective.as_str() {
                "first" => assert!((r.camera[2] - eye_z).abs() < 0.01, "{r:?}"),
                "back" => assert!(r.camera[2] < eye_z - 2.0, "behind the player: {r:?}"),
                _ => assert!(r.camera[2] > eye_z + 2.0, "in front of the player: {r:?}"),
            }
        }
    }
}

/// A mod that rolls the level's bob pose (like Camera Overhaul) rolls mcr's view; vanilla's own bob
/// pose reports no roll.
#[test]
fn pose_rolls_reach_mcrs_view() {
    let Some(live) = live() else {
        return;
    };
    for (props, want) in [(&["-Dmcr.testmod.poseRoll=true"][..], 10.0), (&[][..], 0.0)] {
        let dir = prepare(&format!("pose-roll-{want}"));
        live.send(&STAGE.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>());
        let script = "wait 200\nreport\npress w\nwait 8\nreport\nrelease w\nwait 2\n";
        let lines = run_client_with(&live, &dir, script, props, |line| {
            if line.starts_with("ready:") {
                join_commands()
            } else {
                Vec::new()
            }
        });
        let r = reports(&lines);
        eprintln!("{props:?}: {r:?}");
        assert_eq!(r.len(), 2, "{r:?}");
        for r in &r {
            assert!(
                (r.view_roll - want).abs() < 0.05,
                "{props:?}: walking or standing: {r:?}"
            );
            assert_eq!(r.java_camera, "none", "{r:?}");
        }
    }
}

/// A mod that cancels vanilla's weather extraction (like Particle Rain) hides mcr's rain and snow.
#[test]
fn replaced_weather_hides_mcrs_rain() {
    let Some(live) = live() else {
        return;
    };
    let mut stage: Vec<String> = STAGE.iter().map(|s| (*s).to_owned()).collect();
    stage.push("weather rain 100000".into());
    for (props, replaced) in [
        (&["-Dmcr.testmod.replaceWeather=true"][..], true),
        (&[][..], false),
    ] {
        let dir = prepare(&format!("weather-{replaced}"));
        live.send(&stage);
        let lines = run_client_with(&live, &dir, "wait 200\nreport\n", props, |line| {
            if line.starts_with("ready:") {
                join_commands()
            } else {
                Vec::new()
            }
        });
        let r = reports(&lines);
        assert_eq!(r.len(), 1, "{r:?}");
        assert_eq!(r[0].java_weather, replaced, "{props:?}: {r:?}");
    }
    live.send(&["weather clear 100000".into()]);
}

/// A mod hooked inside `Camera.alignWithEntity` that takes the mouse's turning for its camera (like
/// Freelook) turns mcr's camera while the player keeps its rotation; without it the mouse turns the player.
#[test]
fn freelook_hooks_turn_the_camera_not_the_player() {
    let Some(live) = live() else {
        return;
    };
    let script = "wait 200\nreport\nmotion 300 0\nwait 10\nreport\nmotion 300 0\nwait 10\nreport\n";
    for (props, freelook) in [
        (&["-Dmcr.testmod.freelook=true"][..], true),
        (&[][..], false),
    ] {
        let dir = prepare(&format!("freelook-{freelook}"));
        live.send(&STAGE.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>());
        let lines = run_client_with(&live, &dir, script, props, |line| {
            if line.starts_with("ready:") {
                join_commands()
            } else {
                Vec::new()
            }
        });
        let r = reports(&lines);
        assert_eq!(r.len(), 3, "{r:?}");
        let yaws: Vec<f64> = lines
            .iter()
            .filter(|l| l.starts_with("report:"))
            .filter_map(|l| {
                l.split(" yaw ")
                    .nth(1)?
                    .split_whitespace()
                    .next()?
                    .parse()
                    .ok()
            })
            .collect();
        eprintln!("{props:?}: yaws {yaws:?} reports {r:?}");
        if freelook {
            assert!(
                r.iter().all(|r| r.java_turn),
                "the mod's turn capture: {r:?}"
            );
            assert!(
                yaws.windows(2).all(|w| (w[0] - w[1]).abs() < 0.01),
                "the player turned: {yaws:?}"
            );
            assert_eq!(r[2].java_camera, "moved", "the camera turned: {r:?}");
        } else {
            assert!(
                r.iter().all(|r| !r.java_turn && r.java_camera == "none"),
                "{r:?}"
            );
            assert!(
                (yaws[2] - yaws[0]).abs() > 5.0,
                "the player did not turn: {yaws:?}"
            );
        }
    }
}

/// A mod hooked into vanilla's SDL key events (like malilib's hotkeys) sees mcr's keys and can
/// consume them: with the test mod eating E, the inventory key no longer opens Java's inventory.
#[test]
fn raw_key_hooks_see_and_consume_mcrs_keys() {
    let Some(live) = live() else {
        return;
    };
    let script = "wait 200\ntap e\nwait 10\nreport\ntap escape\nwait 5\n";
    for (props, consumed) in [
        (&["-Dmcr.testmod.rawHotkey=true"][..], true),
        (&[][..], false),
    ] {
        let dir = prepare(&format!("raw-key-{consumed}"));
        live.send(&STAGE.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>());
        let lines = run_client_with(&live, &dir, script, props, |line| {
            if line.starts_with("ready:") {
                join_commands()
            } else {
                Vec::new()
            }
        });
        let r = reports(&lines);
        assert_eq!(r.len(), 1, "{r:?}");
        let want = if consumed { "JavaGame" } else { "JavaScreen" };
        assert_eq!(r[0].mod_input, want, "{props:?}: {r:?}");
    }
}

/// A mod that takes over the player's footsteps (like Presence Footsteps) silences mcr's own.
#[test]
fn replaced_footsteps_silence_mcrs_own() {
    let Some(live) = live() else {
        return;
    };
    for (props, replaced) in [
        (&["-Dmcr.testmod.replaceSteps=true"][..], true),
        (&[][..], false),
    ] {
        let dir = prepare(&format!("steps-{replaced}"));
        live.send(&STAGE.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>());
        let lines = run_client_with(&live, &dir, "wait 200\nreport\n", props, |line| {
            if line.starts_with("ready:") {
                join_commands()
            } else {
                Vec::new()
            }
        });
        let r = reports(&lines);
        assert_eq!(r.len(), 1, "{r:?}");
        assert_eq!(r[0].java_steps, replaced, "{props:?}: {r:?}");
    }
}

/// A mod that lets the movement keys through in screens (like InvMove) moves mcr's player while
/// the inventory is open; without it the player stands still.
#[test]
fn movement_in_screens_moves_mcrs_player() {
    let Some(live) = live() else {
        return;
    };
    let script =
        "wait 200\nreport\ntap e\nwait 10\nhold w 20\nwait 5\nreport\ntap escape\nwait 5\n";
    for (props, moves) in [
        (&["-Dmcr.testmod.moveInScreens=true"][..], true),
        (&[][..], false),
    ] {
        let dir = prepare(&format!("screen-move-{moves}"));
        live.send(&STAGE.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>());
        let lines = run_client_with(&live, &dir, script, props, |line| {
            if line.starts_with("ready:") {
                join_commands()
            } else {
                Vec::new()
            }
        });
        let r = reports(&lines);
        assert_eq!(r.len(), 2, "{r:?}");
        let walked = r[1].pos[2] - r[0].pos[2];
        eprintln!("{props:?}: walked {walked:.2} blocks with the inventory open");
        if moves {
            assert!(walked > 1.0, "the player did not walk: {r:?}");
        } else {
            assert!(
                walked.abs() < 0.01,
                "the player walked without the mod: {r:?}"
            );
        }
    }
}

/// Java frame times with 500 zombies sealed in stone in view, with and without mcr's reached
/// sections culling Java's entities, for the design notes.
#[test]
fn java_frame_time_with_sealed_entities() {
    let Some(live) = live() else {
        return;
    };
    if std::env::var("MCR_SECTION_LOAD").as_deref() != Ok("1") {
        eprintln!("skipped: set MCR_SECTION_LOAD=1");
        return;
    }
    let mut stage: Vec<String> = STAGE.iter().map(|s| (*s).to_owned()).collect();
    // The pocket's sections need a shell of solid sections, or the graph still enters them.
    stage.push("forceload add -32 32 31 79".into());
    for z in (32..80).step_by(10) {
        stage.push(format!("fill -32 128 {z} 31 175 {} stone", (z + 9).min(79)));
    }
    stage.push("fill -12 152 52 11 156 59 air".into());
    for i in 0..500 {
        let (x, z) = ((i % 25) as f64 * 0.9 - 11.5, 52.5 + (i / 25) as f64 * 0.35);
        stage.push(format!(
            "summon zombie {x:.1} 152 {z:.2} {{NoAI:1b,Silent:1b,PersistenceRequired:1b,Rotation:[180f,0f],{HELMET}}}"
        ));
    }
    live.send(&stage);
    for (props, label) in [
        (&["-Dmcr.sections=false"][..], "java culling"),
        (&[][..], "mcr sections"),
    ] {
        let dir = prepare(&format!("sealed-{}", props.len()));
        let lines = run_client_with(&live, &dir, "wait 100\nwait 600\n", props, |line| {
            if line.starts_with("ready:") {
                join_commands()
            } else {
                Vec::new()
            }
        });
        for line in lines
            .iter()
            .filter(|l| l.contains("composed") || l.contains("world_render"))
            .rev()
            .take(3)
        {
            println!("{label}: {line}");
        }
    }
    let mut cleanup = vec!["kill @e[type=!player]".to_owned()];
    for z in (32..80).step_by(10) {
        cleanup.push(format!("fill -32 128 {z} 31 175 {} air", (z + 9).min(79)));
    }
    live.send(&cleanup);
}
