use std::path::PathBuf;
use std::time::{Duration, Instant};

use mcr_audio::{AssetStore, AudioEngine, Listener, SoundManager, SoundSource};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let event = args
        .next()
        .ok_or("usage: play <event> [assets-dir] [x z]")?;
    let root = args.next().map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../.cache/assets"))
    });
    let position = match (args.next(), args.next()) {
        (Some(x), Some(z)) => Some([x.parse()?, 0.0, z.parse()?]),
        _ => None,
    };

    let (engine, handle) = AudioEngine::start(None)?;
    println!(
        "{engine:?}, buffer latency {:?} ms",
        engine.buffer_latency_ms()
    );
    let mut manager = SoundManager::new(AssetStore::open_latest(root)?, handle)?;
    manager.set_listener(Listener::default());

    let started = Instant::now();
    let seed = started.elapsed().as_nanos() as i64 ^ std::process::id() as i64;
    let id = manager.play(&event, position, 1.0, 1.0, SoundSource::Master, seed);
    println!("play -> {id:?} in {:?}", started.elapsed());
    id.ok_or("event unknown or sound unavailable")?;

    std::thread::sleep(Duration::from_millis(100));
    let deadline = Instant::now() + Duration::from_secs(60);
    while manager.handle().active_voices() > 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    Ok(())
}
