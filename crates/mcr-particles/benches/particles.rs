use criterion::{Criterion, criterion_group, criterion_main};
use mcr_assets::ResourceManager;
use mcr_particles::{NoAssets, OpenWorld, PARTICLE_NAMES, ParticleAtlas, ParticleEngine};
use mcr_protocol::types::particle::Particle;
use mcr_render::Environment;
use mcr_render::glam::DVec3;

fn filled() -> Option<ParticleEngine> {
    let jar = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.cache/versions/26.3/client.jar");
    let resources = ResourceManager::vanilla(jar).ok()?;
    let atlas = ParticleAtlas::load(&|p| resources.read(p).ok().flatten());
    let mut engine = ParticleEngine::new(atlas, Box::new(NoAssets));
    let ids: Vec<i32> = [
        "smoke", "flame", "crit", "poof", "cloud", "end_rod", "heart", "note",
    ]
    .iter()
    .map(|n| PARTICLE_NAMES.iter().position(|p| p == n).unwrap() as i32)
    .collect();
    for _ in 0..40 {
        for &id in &ids {
            engine.emit(&Particle::Simple(id), [0.0, 64.0, 0.0], [3.0; 3], 0.05, 40);
        }
    }
    Some(engine)
}

fn bench(c: &mut Criterion) {
    let Some(mut engine) = filled() else { return };
    let env = Environment::default();
    c.bench_function("tick 10k particles", |b| {
        b.iter(|| {
            engine.tick(&OpenWorld);
            if engine.count() < 10_000 {
                for _ in 0..8 {
                    engine.emit(&Particle::Simple(0), [0.0, 64.0, 0.0], [3.0; 3], 0.05, 1000);
                }
            }
        })
    });
    c.bench_function("build instances 10k particles", |b| {
        b.iter(|| engine.instance_count(DVec3::new(0.0, 64.0, 10.0), 0.5, &env))
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);
