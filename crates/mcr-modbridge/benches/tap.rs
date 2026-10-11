use std::hint::black_box;
use std::io::Cursor;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use mcr_modbridge::NetLink;
use mcr_net::{BufPool, FrameDecoder, FrameEncoder};

const ID: i32 = 0x30;
const THRESHOLD: i32 = 256;

fn sample(n: usize) -> Vec<u8> {
    let mut x = 0x2545_f491_4f6c_dd1du64;
    (0..n)
        .map(|i| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            if i % 5 == 0 {
                (x & 0x0f) as u8
            } else {
                (i / 64) as u8
            }
        })
        .collect()
}

/// The reader thread's own work for one frame, the cost the tap is measured against.
fn bench_decode(c: &mut Criterion, size: usize) {
    let payload = sample(size);
    let mut enc = FrameEncoder::new();
    enc.set_compression(THRESHOLD);
    let mut wire = Vec::new();
    enc.encode(ID, &payload, &mut wire).unwrap();
    enc.seal(&mut wire);
    let mut g = c.benchmark_group("reader");
    g.throughput(Throughput::Bytes(size as u64));
    g.bench_function(format!("decode-zlib-{size}"), |b| {
        let mut dec = FrameDecoder::new(BufPool::new(8));
        dec.set_compression(THRESHOLD);
        b.iter(|| {
            let mut r = Cursor::new(&wire);
            dec.fill(&mut r).unwrap();
            black_box(dec.next_frame().unwrap().unwrap().payload.len())
        });
    });
    g.finish();
}

/// The tap copying the decoded frame into the 32 MiB ring while a Java stand-in drains it.
fn bench_tap(c: &mut Criterion, size: usize) {
    let (link, mut java) = NetLink::standalone(32 << 20).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let drainer = {
        let stop = stop.clone();
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                while let Ok(Some(m)) = java.try_pop() {
                    black_box(m.payload().len());
                }
                std::hint::spin_loop();
            }
        })
    };
    let mut hooks = link.hooks("localhost", 25565);
    let tap = hooks.frame_tap.as_mut().unwrap();
    let payload = sample(size);
    let mut g = c.benchmark_group("reader");
    g.throughput(Throughput::Bytes(size as u64));
    g.bench_function(format!("tap-{size}"), |b| {
        b.iter(|| tap(ID, black_box(&payload)))
    });
    g.finish();
    eprintln!("tap-{size}: spill peak {} B", link.stats().spill_peak_bytes);
    stop.store(true, Ordering::Relaxed);
    drainer.join().unwrap();
}

fn benches(c: &mut Criterion) {
    for size in [64, 1024, 16 * 1024] {
        bench_decode(c, size);
        bench_tap(c, size);
    }
}

criterion_group!(group, benches);
criterion_main!(group);
