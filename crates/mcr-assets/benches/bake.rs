#[path = "../tests/common/mod.rs"]
mod common;

use criterion::{Criterion, criterion_group, criterion_main};
use mcr_assets::ModelBaker;

fn bench_bake_all(c: &mut Criterion) {
    if !common::jar_path().exists() || !common::report_path().exists() {
        eprintln!("client jar or blocks report missing, skipping bake benchmarks");
        return;
    }
    let rm = common::vanilla();
    let blocks = common::all_block_states();

    c.bench_function("bake_all_block_states_cold", |b| {
        b.iter(|| {
            let mut baker = ModelBaker::new(&rm);
            let mut quads = 0;
            for block in &blocks {
                for props in &block.states {
                    quads += baker
                        .bake_block_state(&block.name, &common::as_pairs(props))
                        .map_or(0, |m| m.quad_count());
                }
            }
            quads
        })
    });

    let mut warm = ModelBaker::new(&rm);
    for block in &blocks {
        for props in &block.states {
            let _ = warm.bake_block_state(&block.name, &common::as_pairs(props));
        }
    }
    c.bench_function("bake_all_block_states_warm", |b| {
        b.iter(|| {
            let mut quads = 0;
            for block in &blocks {
                for props in &block.states {
                    quads += warm
                        .bake_block_state(&block.name, &common::as_pairs(props))
                        .map_or(0, |m| m.quad_count());
                }
            }
            quads
        })
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(10);
    targets = bench_bake_all
}
criterion_main!(benches);
