use std::hint::black_box;
use std::sync::Arc;

use criterion::{BatchSize, Criterion, Throughput, criterion_group, criterion_main};
use mcr_audio::{
    Command, Mixer, Pcm, PlayRequest, SoundSource, Source, Spatial, VoiceId, pushed_stream,
};

const RATE: u32 = 48_000;
const VOICES: u32 = 64;
const FRAMES: usize = 1024;

fn tone(frames: usize) -> Vec<f32> {
    (0..frames).map(|i| (i as f32 * 0.05).sin() * 0.1).collect()
}

fn spatial(i: u32) -> Spatial {
    Spatial::Positioned {
        position: [i as f32 * 0.25, 1.0, 3.0],
        max_distance: 48.0,
    }
}

fn mixer_with(sources: impl Iterator<Item = Source>) -> Mixer {
    let mut mixer = Mixer::new(RATE);
    for (i, source) in (0..VOICES).zip(sources) {
        let req = PlayRequest {
            spatial: spatial(i),
            pitch: 0.9 + (i % 5) as f32 * 0.05,
            ..PlayRequest::new(source, SoundSource::Blocks)
        };
        mixer.apply(Command::Play(VoiceId(i), req));
    }
    mixer
}

fn bench(c: &mut Criterion) {
    let pcm = Arc::new(Pcm::mono(22_050, tone(22_050)));
    let samples = tone(FRAMES * 2);
    let mut group = c.benchmark_group("mixer_64_voices");
    group.throughput(Throughput::Elements(FRAMES as u64));
    let mut out = vec![0.0f32; FRAMES * 2];

    group.bench_function("static", |b| {
        b.iter_batched_ref(
            || mixer_with((0..VOICES).map(|_| Source::Static(pcm.clone()))),
            |mixer| mixer.render(black_box(&mut out)),
            BatchSize::LargeInput,
        );
    });

    group.bench_function("pushed", |b| {
        b.iter_batched_ref(
            || {
                let mut keep = Vec::new();
                let sources = (0..VOICES).map(|_| {
                    let (mut tx, rx) = pushed_stream(22_050, 1, FRAMES * 2, Arc::default());
                    tx.push(FRAMES * 2, samples.iter().copied());
                    keep.push(tx);
                    Source::Stream(rx)
                });
                let mixer = mixer_with(sources.collect::<Vec<_>>().into_iter());
                (mixer, keep)
            },
            |(mixer, _)| mixer.render(black_box(&mut out)),
            BatchSize::LargeInput,
        );
    });
    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
