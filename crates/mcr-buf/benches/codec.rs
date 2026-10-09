use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use mcr_buf::{BlockPos, Decode, Encode, to_bytes};

#[derive(Encode, Decode)]
struct Move {
    entity: u32,
    #[mcr(varint)]
    dx: i32,
    #[mcr(varint)]
    dy: i32,
    on_ground: bool,
    pos: BlockPos,
}

fn codec(c: &mut Criterion) {
    let value = Move {
        entity: 77,
        dx: -300,
        dy: 5,
        on_ground: true,
        pos: BlockPos::new(-123, 64, 4567),
    };
    let bytes = to_bytes(&value);
    let mut buf = Vec::with_capacity(64);

    c.bench_function("derive/encode", |b| {
        b.iter(|| {
            buf.clear();
            black_box(&value).encode(&mut buf);
        })
    });
    c.bench_function("derive/decode", |b| {
        b.iter(|| Move::decode(&mut black_box(bytes.as_slice())).unwrap())
    });
    c.bench_function("block_pos/pack_unpack", |b| {
        b.iter(|| BlockPos::unpack(black_box(value.pos).pack()))
    });
}

criterion_group!(benches, codec);
criterion_main!(benches);
