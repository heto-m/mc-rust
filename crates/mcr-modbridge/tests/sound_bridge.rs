//! Opt-in (`MCR_JVM_TESTS=1`): Java's sound engine at the title screen, played by an offline mcr-audio mixer
//! paced in real time. The test mod plays its own OGG tone, then menu music streams for 30 s, then everything
//! stops the way `/stopsound` does.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use mcr_audio::{Mixer, VOICE_PAUSED, VOICE_PLAYING, decode_ogg};
use mcr_modbridge::sound::{SOUND_SLOTS, SoundLink, VoiceStart};
use mcr_modbridge::{Bridge, BridgeConfig, BridgeEvent, LaunchManifest, ShutdownOutcome};

const RATE: u32 = 48_000;
const BLOCK: usize = 512;
const DEADLINE: Duration = Duration::from_secs(300);
const MUSIC_SOURCE: usize = 1;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn tone() -> Vec<f32> {
    let path = repo().join("java/mcr-testmod/resources/assets/mcr_testmod/sounds/tone.ogg");
    let pcm = decode_ogg(std::fs::read(path).unwrap()).unwrap();
    assert_eq!((pcm.channels, pcm.sample_rate), (1, RATE));
    pcm.samples.to_vec()
}

/// Left channel of everything the mixer rendered, paced like a sound card.
struct Capture {
    left: Arc<Mutex<Vec<f32>>>,
    active: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Capture {
    fn start(link: &SoundLink) -> Self {
        let mut mixer = Mixer::new(RATE);
        mixer.attach_producer(Box::new(link.voices(RATE).expect("first attach")));
        let left = Arc::new(Mutex::new(Vec::new()));
        let active = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let (l, a, s) = (left.clone(), active.clone(), stop.clone());
        let thread = std::thread::spawn(move || {
            let start = Instant::now();
            let mut rendered = 0u64;
            let mut buf = vec![0.0f32; BLOCK * 2];
            while !s.load(Ordering::Acquire) {
                mixer.render(&mut buf);
                rendered += BLOCK as u64;
                l.lock()
                    .unwrap()
                    .extend(buf.as_chunks::<2>().0.iter().map(|f| f[0]));
                a.store(mixer.active_voices(), Ordering::Release);
                let due = Duration::from_secs_f64(rendered as f64 / f64::from(RATE));
                if let Some(wait) = due.checked_sub(start.elapsed()) {
                    std::thread::sleep(wait);
                }
            }
        });
        Self {
            left,
            active,
            stop,
            thread: Some(thread),
        }
    }

    fn len(&self) -> usize {
        self.left.lock().unwrap().len()
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn correlation(a: &[f32], b: &[f32]) -> f32 {
    let dot: f64 = a
        .iter()
        .zip(b)
        .map(|(x, y)| f64::from(*x) * f64::from(*y))
        .sum();
    let na: f64 = a.iter().map(|x| f64::from(*x).powi(2)).sum();
    let nb: f64 = b.iter().map(|x| f64::from(*x).powi(2)).sum();
    (dot / (na.sqrt() * nb.sqrt()).max(1e-12)) as f32
}

/// Best normalized correlation of `tone` against `mix` around the first sample that rises above silence.
fn best_correlation(mix: &[f32], tone: &[f32]) -> f32 {
    let Some(onset) = mix.iter().position(|s| s.abs() > 1e-3) else {
        return 0.0;
    };
    let lo = onset.saturating_sub(2048);
    let hi = (onset + 2048).min(mix.len().saturating_sub(tone.len()));
    (lo..=hi)
        .map(|lag| correlation(&mix[lag..lag + tone.len()], tone))
        .fold(0.0, f32::max)
}

fn openal_mapped() -> bool {
    std::fs::read_to_string("/proc/self/maps").is_ok_and(|m| m.contains("libopenal"))
}

#[test]
fn java_sounds_play_through_mcr_audio() {
    if std::env::var("MCR_JVM_TESTS").as_deref() != Ok("1") {
        eprintln!("skipped: set MCR_JVM_TESTS=1 to boot Fabric");
        return;
    }
    let cache = mcr_jvm::dev_cache_dir();
    let manifest = LaunchManifest::load(&cache).expect("run `cargo xtask fabric-fetch`");
    let game_dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("sound-bridge");
    let _ = std::fs::remove_dir_all(&game_dir);
    std::fs::create_dir_all(&game_dir).unwrap();
    std::fs::write(
        game_dir.join("options.txt"),
        "onboardAccessibility:false\nsoundCategory_music:0.0\n",
    )
    .unwrap();
    let mut mods = manifest.test_mods.clone();
    mods.push(cache.join("java/mcr-testmod.jar"));
    let mut cfg = BridgeConfig::new(cache, game_dir, mods);
    cfg.max_heap = "1536m".into();
    cfg.extra_jvm = vec!["-Dmcr.testmod.sound=true".into()];

    let link = SoundLink::get().unwrap();
    let capture = Capture::start(&link);
    let mut bridge = Bridge::start(cfg).expect("bridge start");
    let started = Instant::now();
    let mut info: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut voices: Vec<(String, VoiceStart)> = Vec::new();
    let mut phase = String::from("boot");
    let mut marks: BTreeMap<String, usize> = BTreeMap::new();
    let mut underruns_at_music_end = None;
    loop {
        assert!(
            started.elapsed() < DEADLINE,
            "timed out in {phase}: {info:#?}"
        );
        let mut fresh = Vec::new();
        link.take_started(&mut fresh);
        voices.extend(fresh.into_iter().map(|v| (phase.clone(), v)));
        let Some(event) = bridge.next_event(Duration::from_millis(100)) else {
            continue;
        };
        match &event {
            BridgeEvent::Crash { report } => panic!("java crashed: {report:?}"),
            BridgeEvent::VmExited { code } => panic!("java exited with {code}"),
            BridgeEvent::Info { key, value } => {
                info.entry(key.clone()).or_default().push(value.clone());
                if key == "testmod_sound" {
                    phase = value.clone();
                    marks.insert(value.clone(), capture.len());
                }
                if key == "testmod_music_playing" {
                    underruns_at_music_end = Some(link.stats().underruns);
                }
                if key == "testmod_channels" && phase == "stopped" {
                    break;
                }
            }
            _ => {}
        }
    }
    std::thread::sleep(Duration::from_millis(300));
    let mut fresh = Vec::new();
    link.take_started(&mut fresh);
    voices.extend(fresh.into_iter().map(|v| (phase.clone(), v)));
    let active_after_stop = capture.active.load(Ordering::Acquire);
    let playing_slots: Vec<usize> = (0..SOUND_SLOTS)
        .filter(|&s| {
            matches!(link.slot_state(s), Some((_, st)) if st == VOICE_PLAYING || st == VOICE_PAUSED)
        })
        .collect();
    let volumes = link.java_volumes();
    let stats = link.stats();
    let mix = capture.left.lock().unwrap().clone();
    drop(capture);
    let outcome = bridge.shutdown(mcr_modbridge::SHUTDOWN_TIMEOUT);

    eprintln!("voices: {voices:?}\nstats: {stats:?}\ninfo: {info:#?}");
    let static_voices: Vec<_> = voices.iter().filter(|(_, v)| v.buffer.is_some()).collect();
    assert_eq!(static_voices.len(), 1, "the tone starts exactly one voice");
    assert_eq!(static_voices[0].0, "tone");
    let window = &mix[marks["tone"]..marks["music"]];
    let corr = best_correlation(window, &tone());
    eprintln!("tone correlation {corr:.4}");
    assert!(corr >= 0.95, "tone correlation {corr}");
    assert_eq!(
        info["testmod_channels"][0].split(" + ").next(),
        Some("Sounds: 0/247"),
        "the ended tone's channel went back to its pool"
    );

    let streams: Vec<_> = voices.iter().filter(|(_, v)| v.buffer.is_none()).collect();
    assert!(!streams.is_empty(), "menu music never started a stream");
    assert_eq!(info["testmod_music_playing"], ["true"]);
    assert_eq!(underruns_at_music_end, Some(0), "music stream underran");
    let (_, v) = volumes.expect("Java shared its sliders");
    assert_eq!(v[MUSIC_SOURCE], 0.0);
    assert_eq!(v[0], 1.0);

    assert_eq!(
        info["testmod_channels"].last().unwrap(),
        "Sounds: 0/247 + 0/8"
    );
    assert!(
        playing_slots.is_empty(),
        "slots still playing after stop: {playing_slots:?}"
    );
    assert_eq!(active_after_stop, 0, "mixer voices left after stop");
    assert!(
        !info.contains_key("openal_open_device"),
        "OpenAL device opened"
    );
    assert!(!openal_mapped(), "libopenal is mapped into the process");
    assert_eq!(stats.dropped, 0);
    assert_eq!(stats.bad_commands, 0);
    assert!(
        matches!(outcome, ShutdownOutcome::Clean { .. }),
        "shutdown: {outcome:?}"
    );
}
