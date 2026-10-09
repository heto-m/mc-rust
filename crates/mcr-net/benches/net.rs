use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use mcr_net::{BufPool, Deflater, FrameDecoder, FrameEncoder, Inflater};
use std::hint::black_box;
use std::io::{Cursor, Read, Write};

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

fn wire(frames: usize, size: usize, threshold: Option<i32>, key: Option<[u8; 16]>) -> Vec<u8> {
    let payload = sample(size);
    let mut enc = FrameEncoder::new();
    if let Some(t) = threshold {
        enc.set_compression(t);
    }
    if let Some(k) = key {
        enc.set_encryption(&k, 0);
    }
    let mut out = Vec::new();
    for i in 0..frames {
        enc.encode(i as i32 & 0x7f, &payload, &mut out).unwrap();
    }
    enc.seal(&mut out);
    out
}

fn decode_all(dec: &mut FrameDecoder, bytes: &[u8]) -> usize {
    let mut r = Cursor::new(bytes);
    let mut n = 0;
    loop {
        while let Some(f) = dec.next_frame().unwrap() {
            n += f.payload.len();
        }
        if dec.fill(&mut r).is_err() {
            return n;
        }
    }
}

fn bench_decode(c: &mut Criterion) {
    let key = *b"0123456789abcdef";
    type Case = (&'static str, usize, usize, Option<i32>, Option<[u8; 16]>);
    let cases: [Case; 5] = [
        ("small-plain", 4096, 64, None, None),
        ("small-plain-aes", 4096, 64, None, Some(key)),
        ("chunk-plain", 256, 16 * 1024, None, None),
        ("chunk-zlib", 256, 16 * 1024, Some(256), None),
        ("chunk-zlib-aes", 256, 16 * 1024, Some(256), Some(key)),
    ];
    let mut g = c.benchmark_group("decode");
    for (name, frames, size, threshold, secret) in cases {
        let bytes = wire(frames, size, threshold, secret);
        g.throughput(Throughput::Bytes((frames * size) as u64));
        g.bench_function(name, |b| {
            let mut dec = FrameDecoder::new(BufPool::new(64));
            if let Some(t) = threshold {
                dec.set_compression(t);
            }
            if let Some(k) = secret {
                dec.set_encryption(&k);
            }
            b.iter(|| {
                if let Some(k) = secret {
                    dec.set_encryption(&k);
                }
                black_box(decode_all(&mut dec, &bytes))
            });
        });
    }
    g.finish();
}

fn bench_backends(c: &mut Criterion) {
    let mut g = c.benchmark_group("zlib");
    for size in [512usize, 16 * 1024, 256 * 1024] {
        let data = sample(size);
        g.throughput(Throughput::Bytes(size as u64));

        let mut d = Deflater::new();
        let mut packed = Vec::new();
        g.bench_function(format!("libdeflater/compress/{size}"), |b| {
            b.iter(|| d.deflate(black_box(&data), &mut packed).unwrap());
        });
        d.deflate(&data, &mut packed).unwrap();
        let mut inf = Inflater::new();
        let mut out = Vec::new();
        g.bench_function(format!("libdeflater/decompress/{size}"), |b| {
            b.iter(|| inf.inflate(black_box(&packed), size, &mut out).unwrap());
        });

        g.bench_function(format!("zlib-rs/compress/{size}"), |b| {
            b.iter(|| {
                let mut e = ZlibEncoder::new(Vec::new(), Compression::new(6));
                e.write_all(black_box(&data)).unwrap();
                e.finish().unwrap()
            });
        });
        let mut out2 = vec![0u8; size];
        g.bench_function(format!("zlib-rs/decompress/{size}"), |b| {
            b.iter(|| {
                ZlibDecoder::new(black_box(&packed[..]))
                    .read_exact(&mut out2)
                    .unwrap()
            });
        });
    }
    g.finish();
}

criterion_group!(benches, bench_decode, bench_backends);
criterion_main!(benches);
