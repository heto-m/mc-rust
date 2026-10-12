use criterion::{Criterion, criterion_group, criterion_main};
use mcr_animate::{AnimateWorld, Animator, Cue, Sink};
use mcr_protocol::types::particle::Particle;

struct Dense(u16);

impl AnimateWorld for Dense {
    fn state(&self, x: i32, y: i32, z: i32) -> Option<u16> {
        let torch = (x ^ y ^ z) & 7 == 0;
        Some(if torch { self.0 } else { 0 })
    }
}

struct Null;

impl Sink for Null {
    fn particle(&mut self, _: &Particle, _: [f64; 3], _: [f64; 3], _: bool) {}
    fn sound(&mut self, _: &Cue) {}
}

fn bench(c: &mut Criterion) {
    let data = mcr_data::v26_3();
    let mut animator = Animator::new(data, 1);
    let torch = data
        .block_by_name("minecraft:torch")
        .and_then(|b| data.block_info(b))
        .map_or(0, |b| b.default_state as u16);
    let world = Dense(torch);
    c.bench_function("animate_tick_dense", |b| {
        b.iter(|| animator.tick([0, 64, 0], &world, &mut Null, true));
    });
    c.bench_function("animate_tick_empty", |b| {
        b.iter(|| animator.tick([0, 64, 0], &Dense(0), &mut Null, true));
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);
