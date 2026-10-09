use std::hint::black_box;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use mcr_buf::to_bytes;
use mcr_nbt::{Compound, Limits, List, NetworkNbt, Value, raw, read_network};

fn big_compound() -> Compound {
    let mut root = Compound::new();
    let mut sections = Vec::new();
    for i in 0..24 {
        let mut s = Compound::new();
        s.insert("Y", i as i8);
        s.insert(
            "palette",
            List::String((0..16).map(|n| format!("minecraft:block_{n}")).collect()),
        );
        s.insert("data", vec![i64::from(i); 256]);
        sections.push(s);
    }
    root.insert("sections", List::Compound(sections));
    let mut hm = Compound::new();
    hm.insert("MOTION_BLOCKING", vec![0x0123_4567_89ab_cdefi64; 37]);
    hm.insert("WORLD_SURFACE", vec![0x0123_4567_89ab_cdefi64; 37]);
    root.insert("Heightmaps", hm);
    for i in 0..64 {
        root.insert(format!("key_{i}"), format!("value number {i}"));
    }
    root.insert("last", 7i32);
    root
}

fn bench(c: &mut Criterion) {
    let bytes = to_bytes(&NetworkNbt(Value::Compound(big_compound())));
    let mut g = c.benchmark_group("nbt");
    g.throughput(Throughput::Bytes(bytes.len() as u64));
    g.bench_function("decode_owned", |b| {
        b.iter(|| read_network(&mut black_box(&bytes[..]), Limits::UNLIMITED).unwrap())
    });
    g.bench_function("raw_extract_heightmap", |b| {
        b.iter(|| {
            let root = raw::network_root(black_box(&bytes)).unwrap().unwrap();
            root.as_compound()
                .unwrap()
                .get_compound("Heightmaps")
                .unwrap()
                .get_long_array("MOTION_BLOCKING")
                .unwrap()
                .get(3)
        })
    });
    g.bench_function("raw_skip_all", |b| {
        b.iter(|| {
            raw::network_root(black_box(&bytes))
                .unwrap()
                .unwrap()
                .size()
                .unwrap()
        })
    });
    g.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
