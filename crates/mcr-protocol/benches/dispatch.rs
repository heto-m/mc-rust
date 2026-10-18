use std::fs;
use std::hint::black_box;
use std::path::Path;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use mcr_protocol::dispatch::PlayClientbound;
use mcr_protocol::version::{Session, Translated, Wire};
use mcr_protocol::{PacketIds, ProtocolState};

fn captured_play_frames() -> Vec<(i32, Vec<u8>)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.cache/captures/26.3");
    let Ok(index) = fs::read_to_string(dir.join("index.jsonl")) else {
        return Vec::new();
    };
    index
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|e| e["state"] == "play" && e["direction"] == "clientbound")
        .map(|e| {
            let path = dir
                .join("play/clientbound")
                .join(e["name"].as_str().unwrap())
                .join(format!("{}.bin", e["seq"]));
            (e["id"].as_i64().unwrap() as i32, fs::read(path).unwrap())
        })
        .collect()
}

fn play_stream(c: &mut Criterion) {
    let frames = captured_play_frames();
    if frames.is_empty() {
        eprintln!("skipping: no captures in .cache/captures/26.3");
        return;
    }
    let ids = PacketIds::v26_3();
    let bytes: usize = frames.iter().map(|(_, p)| p.len()).sum();
    let mut group = c.benchmark_group("play stream");
    group.throughput(Throughput::Bytes(bytes as u64));
    group.bench_function("decode", |b| {
        b.iter(|| {
            for (id, payload) in &frames {
                black_box(PlayClientbound::decode(*id, payload, ids).unwrap());
            }
        });
    });
    let mut scratch = Vec::new();
    let mut session = Session::default();
    group.bench_function("decode via canonical wire", |b| {
        b.iter(|| {
            for (id, payload) in &frames {
                let wire = black_box(Wire::Canonical);
                if let Ok(Translated::Packet(id, payload)) = wire.inbound(
                    ProtocolState::Play,
                    *id,
                    payload,
                    &mut session,
                    &mut scratch,
                ) {
                    black_box(PlayClientbound::decode(id, payload, ids).unwrap());
                }
            }
        });
    });
    let packets: Vec<_> = frames
        .iter()
        .map(|(id, payload)| PlayClientbound::decode(*id, payload, ids).unwrap())
        .collect();
    let mut out = Vec::with_capacity(bytes);
    group.bench_function("encode", |b| {
        b.iter(|| {
            for packet in &packets {
                out.clear();
                black_box(packet.encode_into(ids, &mut out));
            }
        });
    });
    group.finish();
}

criterion_group!(benches, play_stream);
criterion_main!(benches);
