use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use mcr_buf::{Decode, Encode};
use mcr_protocol::play::serverbound::MovePlayerPosRot;

fn move_player_pos_rot(c: &mut Criterion) {
    let packet = MovePlayerPosRot::new(104.5, 64.0, -12.25, 90.0, 15.0, true, false);
    let mut out = Vec::with_capacity(64);
    c.bench_function("encode move_player_pos_rot", |b| {
        b.iter(|| {
            out.clear();
            black_box(&packet).encode(&mut out);
            black_box(&out);
        });
    });
    let bytes = mcr_buf::to_bytes(&packet);
    c.bench_function("decode move_player_pos_rot", |b| {
        b.iter(|| MovePlayerPosRot::decode(&mut black_box(bytes.as_slice())).unwrap());
    });
}

criterion_group!(benches, move_player_pos_rot);
criterion_main!(benches);
