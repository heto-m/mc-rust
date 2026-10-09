use std::path::PathBuf;

use mcr_audio::{AssetStore, Mixer, SoundManager, SoundSource};

fn store() -> Option<AssetStore> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.cache/assets");
    AssetStore::open_latest(root).ok()
}

fn rms(buf: &[f32]) -> f32 {
    (buf.iter().map(|s| s * s).sum::<f32>() / buf.len() as f32).sqrt()
}

#[test]
fn plays_a_decoded_effect_and_a_streamed_track() {
    let Some(store) = store() else {
        eprintln!("asset store missing, run `cargo xtask assets`");
        return;
    };
    let (mut mixer, handle) = Mixer::with_queue(48_000);
    let mut manager = SoundManager::new(store, handle).unwrap();
    assert!(manager.events().len() > 1000);

    let step = manager.play(
        "block.stone.step",
        Some([2.0, 0.0, 0.0]),
        1.0,
        1.0,
        SoundSource::Blocks,
        7,
    );
    assert!(step.is_some());
    let mut buf = vec![0.0; 48_000 / 4 * 2];
    mixer.render(&mut buf);
    assert!(rms(&buf) > 1e-4, "step produced silence");

    let music = manager.play("music.game", None, 1.0, 1.0, SoundSource::Music, 3);
    assert!(music.is_some());
    let mut heard = false;
    for _ in 0..40 {
        mixer.render(&mut buf);
        heard |= rms(&buf) > 1e-4;
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(heard, "stream produced silence");
}
