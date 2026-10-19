use glam::DVec3;

use super::{
    Data, Emitter, Haze, Kind, LEAVES_LIFE, Liquid, MAX_SPAWNS_PER_TICK, P, Spawn, set_from_age,
    set_size, type_id,
};
use crate::atlas::ParticleAtlas;
use crate::rng::Rng;
use crate::{Fluid, ParticleWorld};

fn aabb(p: &P) -> [f64; 6] {
    let w = f64::from(p.half_w);
    [
        p.pos.x - w,
        p.pos.y,
        p.pos.z - w,
        p.pos.x + w,
        p.pos.y + f64::from(p.height),
        p.pos.z + w,
    ]
}

pub(crate) struct Scratch {
    boxes: Vec<[f64; 6]>,
    local: Vec<[f32; 6]>,
}

impl Scratch {
    pub(crate) fn new() -> Self {
        Self {
            boxes: Vec::with_capacity(64),
            local: Vec::with_capacity(8),
        }
    }
}

fn collide(world: &dyn ParticleWorld, bb: [f64; 6], d: DVec3, s: &mut Scratch) -> DVec3 {
    let lo = [
        bb[0].min(bb[0] + d.x),
        bb[1].min(bb[1] + d.y),
        bb[2].min(bb[2] + d.z),
    ];
    let hi = [
        bb[3].max(bb[3] + d.x),
        bb[4].max(bb[4] + d.y),
        bb[5].max(bb[5] + d.z),
    ];
    s.boxes.clear();
    let cell = |v: f64| v.floor().clamp(-3.0e7, 3.0e7) as i32;
    let (x0, y0, z0) = (cell(lo[0]), cell(lo[1]), cell(lo[2]));
    let (x1, y1, z1) = (cell(hi[0]), cell(hi[1]), cell(hi[2]));
    if (x1 - x0) > 12 || (y1 - y0) > 12 || (z1 - z0) > 12 {
        return d;
    }
    for x in x0..=x1 {
        for y in y0..=y1 {
            for z in z0..=z1 {
                s.local.clear();
                world.collision_boxes(x, y, z, &mut s.local);
                for b in &s.local {
                    s.boxes.push([
                        f64::from(x) + f64::from(b[0]),
                        f64::from(y) + f64::from(b[1]),
                        f64::from(z) + f64::from(b[2]),
                        f64::from(x) + f64::from(b[3]),
                        f64::from(y) + f64::from(b[4]),
                        f64::from(z) + f64::from(b[5]),
                    ]);
                }
            }
        }
    }
    if s.boxes.is_empty() {
        return d;
    }
    let mut bb = bb;
    let mut out = d;
    let clip = |axis: usize, bb: &[f64; 6], boxes: &[[f64; 6]], mut dv: f64| -> f64 {
        let (a, b) = ((axis + 1) % 3, (axis + 2) % 3);
        for o in boxes {
            if bb[a + 3] <= o[a] || bb[a] >= o[a + 3] || bb[b + 3] <= o[b] || bb[b] >= o[b + 3] {
                continue;
            }
            if dv > 0.0 && bb[axis + 3] <= o[axis] {
                dv = dv.min(o[axis] - bb[axis + 3]);
            } else if dv < 0.0 && bb[axis] >= o[axis + 3] {
                dv = dv.max(o[axis + 3] - bb[axis]);
            }
        }
        dv
    };
    let shift = |bb: &mut [f64; 6], axis: usize, dv: f64| {
        bb[axis] += dv;
        bb[axis + 3] += dv;
    };
    out.y = clip(1, &bb, &s.boxes, out.y);
    shift(&mut bb, 1, out.y);
    let x_first = out.x.abs() >= out.z.abs();
    let order = if x_first { [0, 2] } else { [2, 0] };
    for axis in order {
        let v = clip(axis, &bb, &s.boxes, if axis == 0 { out.x } else { out.z });
        shift(&mut bb, axis, v);
        if axis == 0 {
            out.x = v;
        } else {
            out.z = v;
        }
    }
    out
}

fn mv(p: &mut P, world: &dyn ParticleWorld, s: &mut Scratch, d: DVec3) {
    if p.stopped {
        return;
    }
    let orig = d;
    let mut d = d;
    if p.physics && d != DVec3::ZERO && d.length_squared() < 10_000.0 {
        d = collide(world, aabb(p), d, s);
    }
    p.pos += d;
    if orig.y.abs() >= 1.0e-5 && d.y.abs() < 1.0e-5 {
        p.stopped = true;
    }
    p.on_ground = orig.y != d.y && orig.y < 0.0;
    if orig.x != d.x {
        p.vel.x = 0.0;
    }
    if orig.z != d.z {
        p.vel.z = 0.0;
    }
}

pub(crate) struct Tick<'a> {
    pub world: &'a dyn ParticleWorld,
    pub atlas: &'a ParticleAtlas,
    pub rng: &'a mut Rng,
    pub spawns: &'a mut Vec<Spawn>,
    pub scratch: &'a mut Scratch,
}

fn queue(t: &mut Tick, spawn: Spawn) {
    if t.spawns.len() < MAX_SPAWNS_PER_TICK {
        t.spawns.push(spawn);
    }
}

fn drag(p: &mut P, f: f64) {
    p.vel *= f;
}

fn cell(v: f64) -> i32 {
    v.floor().clamp(-3.0e7, 3.0e7) as i32
}

fn cells(p: DVec3) -> (i32, i32, i32) {
    (cell(p.x), cell(p.y), cell(p.z))
}

fn in_water(world: &dyn ParticleWorld, at: DVec3) -> bool {
    let (x, y, z) = cells(at);
    matches!(world.fluid(x, y, z), Some((Fluid::Water, _)))
}

fn spawn(type_id: u16, pos: DVec3, aux: DVec3, data: Data) -> Spawn {
    Spawn {
        type_id,
        pos,
        aux,
        data,
    }
}

/// Vanilla `FireflyParticle#getFadeAmount`.
pub(crate) fn fade(progress: f32, fade_in: f32, fade_out: f32) -> f32 {
    if progress >= 1.0 - fade_in {
        (1.0 - progress) / fade_in
    } else if progress <= fade_out {
        progress / fade_out
    } else {
        1.0
    }
}

/// Advances a particle one tick; returns false when it should be removed.
pub(crate) fn tick(p: &mut P, t: &mut Tick) -> bool {
    if p.kind == Kind::Shriek && p.aux[0] > 0.0 {
        p.aux[0] -= 1.0;
        return true;
    }
    p.prev = p.pos;
    p.prev_roll = p.roll;
    let at = cells(p.pos);
    p.light = t.world.light(at.0, at.1, at.2);
    match p.kind {
        Kind::Seed => {
            for _ in 0..6 {
                let j = |r: &mut Rng| (r.f64() - r.f64()) * 4.0;
                let off = DVec3::new(j(t.rng), j(t.rng), j(t.rng));
                let aux = DVec3::new(f64::from(p.age) / f64::from(p.life), 0.0, 0.0);
                queue(t, spawn(type_id("explosion"), p.pos + off, aux, Data::None));
            }
            p.age += 1;
            return p.age < p.life;
        }
        Kind::Emitter(e) => return tick_emitter(p, t, e),
        Kind::Portal => {
            p.age += 1;
            if p.age >= p.life {
                return false;
            }
            let a = p.age as f32 / p.life as f32;
            let s = 1.0 - (-a + a * a * 2.0);
            let start = p.start;
            p.pos = DVec3::new(
                start.x + p.vel.x * f64::from(s),
                start.y + p.vel.y * f64::from(s) + f64::from(1.0 - a),
                start.z + p.vel.z * f64::from(s),
            );
            return true;
        }
        Kind::ReversePortal => {
            p.age += 1;
            if p.age > p.life {
                return false;
            }
            p.pos += p.vel * (f64::from(p.age) / f64::from(p.life));
            return true;
        }
        Kind::FlyTowards | Kind::VaultConnection => {
            let expired = p.age >= p.life;
            p.age += 1;
            if expired {
                return false;
            }
            let pos = 1.0 - p.age as f32 / p.life as f32;
            let pp = (1.0 - pos).powi(4);
            let start = p.start;
            p.pos = DVec3::new(
                start.x + p.vel.x * f64::from(pos),
                start.y + p.vel.y * f64::from(pos) - f64::from(pp * 1.2),
                start.z + p.vel.z * f64::from(pos),
            );
            return true;
        }
        Kind::FlyStraight => {
            p.age += 1;
            if p.age > p.life {
                return false;
            }
            let f = p.age as f32 / p.life as f32;
            p.pos = p.start + p.vel * f64::from(1.0 - f);
            let from = super::rgb(super::OMINOUS_START);
            for (c, s) in p.color.iter_mut().zip(from) {
                *c = s + (1.0 - s) * f;
            }
            return true;
        }
        Kind::Bubble => {
            p.life -= 1;
            if p.life <= 0 {
                return false;
            }
            p.vel.y += 0.002;
            let d = p.vel;
            mv(p, t.world, t.scratch, d);
            drag(p, 0.85);
            return in_water(t.world, p.pos);
        }
        Kind::CurrentDown => {
            p.age += 1;
            if p.age > p.life {
                return false;
            }
            let angle = p.aux[0];
            p.vel.x = (p.vel.x + f64::from(0.6 * angle.cos())) * 0.07;
            p.vel.z = (p.vel.z + f64::from(0.6 * angle.sin())) * 0.07;
            let d = p.vel;
            mv(p, t.world, t.scratch, d);
            p.aux[0] += 0.08;
            return in_water(t.world, p.pos) && !p.on_ground;
        }
        Kind::BubblePop => {
            p.age += 1;
            if p.age >= p.life {
                return false;
            }
            p.vel.y -= f64::from(p.gravity);
            let d = p.vel;
            mv(p, t.world, t.scratch, d);
            set_from_age(p, t.atlas);
            return true;
        }
        Kind::WaterDrop | Kind::Splash => return tick_rain(p, t),
        Kind::Hang(l) | Kind::Fall(l) | Kind::Land(l) => return tick_drip(p, t, l),
        Kind::Campfire => {
            p.age += 1;
            if p.age >= p.life || p.color[3] <= 0.0 {
                return false;
            }
            let j = |r: &mut Rng| f64::from(r.f32() / 5000.0) * if r.bool() { 1.0 } else { -1.0 };
            p.vel.x += j(t.rng);
            p.vel.z += j(t.rng);
            p.vel.y -= f64::from(p.gravity);
            let d = p.vel;
            mv(p, t.world, t.scratch, d);
            if p.age >= p.life - 60 && p.color[3] > 0.01 {
                p.color[3] -= 0.015;
            }
            return true;
        }
        Kind::Town(_) => {
            p.life -= 1;
            if p.life <= 0 {
                return false;
            }
            p.pos += p.vel;
            drag(p, 0.99);
            return true;
        }
        Kind::Sweep | Kind::Explosion | Kind::SonicBoom | Kind::Gust => {
            p.age += 1;
            if p.age >= p.life {
                return false;
            }
            set_from_age(p, t.atlas);
            return true;
        }
        Kind::Leaves(_) => return tick_leaves(p, t),
        Kind::FallingDust => {
            p.age += 1;
            if p.age > p.life {
                return false;
            }
            set_from_age(p, t.atlas);
            p.roll += std::f32::consts::PI * p.aux[0] * 2.0;
            if p.on_ground {
                p.prev_roll = 0.0;
                p.roll = 0.0;
            }
            let d = p.vel;
            mv(p, t.world, t.scratch, d);
            p.vel.y = (p.vel.y - f64::from(0.003f32)).max(-0.14);
            return true;
        }
        Kind::DragonBreath => {
            p.age += 1;
            if p.age > p.life {
                return false;
            }
            set_from_age(p, t.atlas);
            if p.on_ground {
                p.vel.y = 0.0;
                p.flag = true;
            }
            if p.flag {
                p.vel.y += 0.002;
            }
            let d = p.vel;
            mv(p, t.world, t.scratch, d);
            if p.pos.y == p.prev.y {
                p.vel.x *= 1.1;
                p.vel.z *= 1.1;
            }
            let f = f64::from(p.friction);
            p.vel.x *= f;
            p.vel.z *= f;
            if p.flag {
                p.vel.y *= f;
            }
            return true;
        }
        Kind::Wake => {
            let lived = 60 - p.life;
            p.life -= 1;
            if p.life < 0 {
                return false;
            }
            let d = p.vel;
            mv(p, t.world, t.scratch, d);
            drag(p, 0.98);
            let size = lived as f32 * 0.001;
            set_size(p, size, size);
            let set = t.atlas.set(usize::from(p.type_id));
            if let Some(last) = set.len().checked_sub(1) {
                p.sprite = set[(lived.rem_euclid(4) as usize * last / 4).min(last)];
            }
            return true;
        }
        Kind::Vibration => {
            p.age += 1;
            if p.age > p.life {
                return false;
            }
            let dest = if p.flag {
                match t.world.entity_position(p.extra.x as i32) {
                    Some(e) if e.iter().all(|c| c.is_finite()) => {
                        DVec3::from_array(e) + DVec3::Y * p.extra.y
                    }
                    _ => return false,
                }
            } else {
                p.extra
            };
            if p.aux[4] == 0.0 {
                p.aux[4] = 1.0;
                let (rot, pitch) = aim(p.pos - dest);
                p.aux[0] = rot;
                p.aux[2] = pitch;
            }
            lerp_to(p, dest);
            let (rot, pitch) = aim(p.pos - dest);
            p.aux[1] = p.aux[0];
            p.aux[0] = rot;
            p.aux[3] = p.aux[2];
            p.aux[2] = pitch;
            return true;
        }
        Kind::Trail => {
            p.age += 1;
            if p.age > p.life {
                return false;
            }
            let dest = p.extra;
            lerp_to(p, dest);
            return true;
        }
        _ => {}
    }

    p.age += 1;
    if p.age > p.life {
        return false;
    }
    if p.kind == Kind::Ash(Haze::Plume) {
        p.gravity *= 0.88;
        p.friction *= 0.92;
    }
    p.vel.y -= 0.04 * f64::from(p.gravity);
    let d = p.vel;
    mv(p, t.world, t.scratch, d);
    if p.speed_up && p.pos.y == p.prev.y {
        p.vel.x *= 1.1;
        p.vel.z *= 1.1;
    }
    drag(p, f64::from(p.friction));
    if p.on_ground {
        p.vel.x *= f64::from(0.7f32);
        p.vel.z *= f64::from(0.7f32);
    }
    match p.kind {
        Kind::Ash(style) => {
            set_from_age(p, t.atlas);
            if style == Haze::Noxious && p.age as f32 > p.aux[0] {
                p.color[3] = (p.life as f32 - (p.age as f32 - p.aux[0])) / p.life as f32;
            }
        }
        Kind::Dust
        | Kind::Transition
        | Kind::Cloud
        | Kind::Generic
        | Kind::Poof
        | Kind::Rising
        | Kind::Glow(_)
        | Kind::TrialDetect
        | Kind::Sculk => set_from_age(p, t.atlas),
        Kind::Snowflake => {
            set_from_age(p, t.atlas);
            p.vel.x *= f64::from(0.95f32);
            p.vel.y *= f64::from(0.9f32);
            p.vel.z *= f64::from(0.95f32);
        }
        Kind::Crit => {
            p.color[1] *= 0.96;
            p.color[2] *= 0.9;
            set_from_age(p, t.atlas);
        }
        Kind::LavaPop => {
            let odds = p.age as f32 / p.life as f32;
            if t.rng.f32() > odds {
                queue(t, spawn(type_id("smoke"), p.pos, p.vel, Data::None));
            }
        }
        Kind::SquidInk => {
            set_from_age(p, t.atlas);
            let half = p.life / 2;
            if p.age > half {
                p.color[3] = 1.0 - (p.age - half) as f32 / p.life as f32;
            }
            let (x, y, z) = cells(p.pos);
            if t.world.is_air(x, y, z) {
                p.vel.y -= f64::from(0.0074f32);
            }
        }
        Kind::Firefly => {
            let (x, y, z) = cells(p.pos);
            if !t.world.is_air(x, y, z) {
                return false;
            }
            p.color[3] = fade((p.age as f32 / p.life as f32).clamp(0.0, 1.0), 0.3, 0.5);
            if t.rng.f32() > 0.95 || p.age == 1 {
                let mut r = || f64::from(-0.05 + 0.1 * t.rng.f32());
                p.vel = DVec3::new(r(), r(), r());
            }
        }
        Kind::GeyserPlume => tick_plume(p, t.atlas),
        Kind::SulfurBubble => return tick_sulfur(p, t),
        Kind::BubbleColumn => return in_water(t.world, p.pos),
        Kind::Animated | Kind::Spell => tick_animated(p, t),
        _ => {}
    }
    if p.kind == Kind::Transition {
        let f = p.age as f32 / p.life as f32;
        for i in 0..3 {
            p.color[i] = p.aux[3 + i] + (p.aux[i] - p.aux[3 + i]) * f;
        }
    }
    true
}

/// Yaw and pitch of an offset, as vanilla's vibration signal orients itself.
fn aim(d: DVec3) -> (f32, f32) {
    (
        d.x.atan2(d.z) as f32,
        d.y.atan2((d.x * d.x + d.z * d.z).sqrt()) as f32,
    )
}

/// Moves `1 / remaining` of the way to `dest`, arriving exactly on the last tick.
fn lerp_to(p: &mut P, dest: DVec3) {
    let remaining = p.life - p.age;
    if remaining <= 0 {
        p.pos = dest;
    } else {
        p.pos += (dest - p.pos) / f64::from(remaining);
    }
}

fn tick_emitter(p: &mut P, t: &mut Tick, e: Emitter) -> bool {
    match e {
        Emitter::GustLarge | Emitter::GustSmall => {
            let (scale, delay) = if e == Emitter::GustLarge {
                (3.0, 0)
            } else {
                (1.0, 2)
            };
            if p.age % (delay + 1) == 0 {
                let gust = type_id("gust");
                let aux = DVec3::new(f64::from(p.age) / f64::from(p.life), 0.0, 0.0);
                for _ in 0..3 {
                    let mut j = || (t.rng.f64() - t.rng.f64()) * scale;
                    let off = DVec3::new(j(), j(), j());
                    queue(t, spawn(gust, p.pos + off, aux, Data::None));
                }
            }
            let done = p.age >= p.life;
            p.age += 1;
            !done
        }
        Emitter::Geyser => {
            let alive = p.age < p.life;
            p.age += 1;
            let water = p.aux[0] as i32;
            let data = |impulse| Data::Geyser {
                water_blocks: water,
                impulse,
            };
            if p.age % 2 == 0 {
                let id = type_id("geyser_base");
                for _ in 0..2 {
                    queue(t, spawn(id, p.pos, p.extra, data(1.5)));
                }
            }
            let id = type_id("geyser_plume");
            for _ in 0..water + 2 {
                queue(t, spawn(id, p.pos, p.extra, data(0.0)));
            }
            if p.age % 10 == 0 {
                let id = type_id("geyser_poof");
                for _ in 0..20 {
                    queue(t, spawn(id, p.pos, p.extra, data(2.0)));
                }
            }
            alive
        }
        Emitter::Noxious => {
            let alive = p.age < p.life;
            p.age += 1;
            if p.age % 2 == 0 {
                let center = p.pos.floor() + 0.5;
                let dir = DVec3::new(
                    f64::from(t.rng.f32() - 0.5),
                    0.0,
                    f64::from(t.rng.f32() - 0.5),
                )
                .normalize_or_zero();
                let at = center + dir * f64::from(t.rng.f32() * 3.0) - DVec3::Y * 0.25;
                let (x, y, z) = cells(at);
                t.scratch.local.clear();
                t.world.collision_boxes(x, y, z, &mut t.scratch.local);
                if t.scratch.local.is_empty() {
                    queue(
                        t,
                        spawn(type_id("noxious_gas"), at, DVec3::ZERO, Data::None),
                    );
                }
            }
            alive
        }
    }
}

fn tick_rain(p: &mut P, t: &mut Tick) -> bool {
    p.life -= 1;
    if p.life <= 0 {
        return false;
    }
    p.vel.y -= f64::from(p.gravity);
    let d = p.vel;
    mv(p, t.world, t.scratch, d);
    drag(p, 0.98);
    if p.on_ground {
        if t.rng.f32() < 0.5 {
            return false;
        }
        p.vel.x *= 0.7;
        p.vel.z *= 0.7;
    }
    let (x, y, z) = cells(p.pos);
    let mut top = 0.0f32;
    t.scratch.local.clear();
    t.world.collision_boxes(x, y, z, &mut t.scratch.local);
    let (fx, fz) = (
        (p.pos.x - f64::from(x)) as f32,
        (p.pos.z - f64::from(z)) as f32,
    );
    for b in &t.scratch.local {
        if fx >= b[0] && fx <= b[3] && fz >= b[2] && fz <= b[5] {
            top = top.max(b[4]);
        }
    }
    if let Some((_, h)) = t.world.fluid(x, y, z) {
        top = top.max(h);
    }
    !(top > 0.0 && p.pos.y < f64::from(y) + f64::from(top))
}

/// Vanilla `FallingParticle`: drifts down for up to 300 ticks, spinning and swirling with the wind.
fn tick_leaves(p: &mut P, t: &mut Tick) -> bool {
    if p.life <= 0 {
        return false;
    }
    p.life -= 1;
    let rel = ((LEAVES_LIFE - p.life) as f32 / LEAVES_LIFE as f32).min(1.0);
    let [_, spin, wind, x_flow, z_flow, period] = p.aux;
    let (mut xa, mut za) = (0.0f64, 0.0f64);
    if p.extra.x > 0.5 {
        let k = f64::from(rel).powf(1.25);
        xa += f64::from(x_flow) * k;
        za += f64::from(z_flow) * k;
    }
    if p.flag {
        let a = rel * period;
        xa += f64::from(rel * a.cos() * wind);
        za += f64::from(rel * a.sin() * wind);
    }
    p.vel.x += xa * f64::from(0.0025f32);
    p.vel.z += za * f64::from(0.0025f32);
    p.vel.y -= f64::from(p.gravity);
    p.aux[0] += spin / 20.0;
    p.roll += p.aux[0] / 20.0;
    let d = p.vel;
    mv(p, t.world, t.scratch, d);
    if p.on_ground || (p.life < LEAVES_LIFE - 1 && (p.vel.x == 0.0 || p.vel.z == 0.0)) {
        return false;
    }
    drag(p, f64::from(p.friction));
    true
}

fn tick_plume(p: &mut P, atlas: &ParticleAtlas) {
    let (start, max, propulsion) = (p.extra.x, p.extra.y, p.extra.z as f32);
    if p.aux[4] == 0.0 && (p.vel.y < 0.0 || p.pos.y > max || p.pos.y == p.prev.y) {
        p.life = p.life.min(p.age + 5);
        p.friction = 0.0;
        p.aux[4] = 1.0;
    }
    let span = (max - start).max(1e-3);
    let linear = ((p.pos.y - start) / span).clamp(0.0, 1.0);
    p.gravity = propulsion * linear.powi(3) as f32 * 0.12;
    p.vel.x = linear * f64::from(p.aux[0]);
    p.vel.z = linear * f64::from(p.aux[1]);
    set_from_age(p, atlas);
    p.size = p.aux[2] + linear as f32 * (p.aux[3] - p.aux[2]);
}

/// Vanilla `SulfurBubbleParticle`: rises three blocks through still water, growing as it goes.
fn tick_sulfur(p: &mut P, t: &mut Tick) -> bool {
    let (x, y, z) = cells(p.pos);
    let (start, end, prev) = (p.extra.x, p.extra.y, p.extra.z);
    if !t.world.is_water_source(x, y, z) || p.pos.y >= end || p.pos.y <= prev {
        return false;
    }
    let mut wiggle = || {
        let sign = if t.rng.bool() { 1.0 } else { -1.0 };
        f64::from(t.rng.f32() * 0.003) * sign * 0.5
    };
    p.vel.x += wiggle();
    p.vel.z += wiggle();
    let d = DVec3::new(p.vel.x, 0.0, p.vel.z);
    mv(p, t.world, t.scratch, d);
    let progress = ((p.pos.y - start) / (end - start)) as f32;
    p.size = p.aux[0] + progress * (0.15 - p.aux[0]);
    p.extra.z = p.pos.y;
    true
}

fn tick_animated(p: &mut P, t: &mut Tick) {
    set_from_age(p, t.atlas);
    if p.kind == Kind::Spell {
        p.color[3] += (p.aux[3] - p.color[3]) * 0.05;
    } else if p.age > p.life / 2 {
        p.color[3] = 1.0 - (p.age as f32 - p.life as f32 / 2.0) / p.life as f32;
        if p.aux[3] > 0.5 {
            for i in 0..3 {
                p.color[i] += (p.aux[i] - p.color[i]) * 0.2;
            }
        }
    }
    if p.kind == Kind::Animated && p.aux[5] > 0.5 && p.age < p.life / 2 && (p.age + p.life) % 2 == 0
    {
        let color = p.color.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u32);
        let fade = p.aux.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u32);
        let spark = super::Spark {
            color: color[0] << 16 | color[1] << 8 | color[2],
            fade: (p.aux[3] > 0.5).then_some(fade[0] << 16 | fade[1] << 8 | fade[2]),
            trail: false,
            twinkle: p.aux[4] > 0.5,
            trailed: true,
        };
        queue(
            t,
            spawn(p.type_id, p.pos, DVec3::ZERO, Data::Firework(spark)),
        );
    }
}

fn tick_drip(p: &mut P, t: &mut Tick, l: Liquid) -> bool {
    let hanging = matches!(p.kind, Kind::Hang(_));
    if hanging && matches!(l, Liquid::Lava | Liquid::DripLava) {
        p.color[0] = 1.0;
        p.color[1] = 16.0 / (40 - p.life + 16) as f32;
        p.color[2] = 4.0 / (40 - p.life + 8) as f32;
    }
    p.life -= 1;
    if p.life <= 0 {
        if hanging {
            let name = match l {
                Liquid::Water => "falling_water",
                Liquid::Lava => "falling_lava",
                Liquid::Honey => "falling_honey",
                Liquid::Obsidian => "falling_obsidian_tear",
                Liquid::DripWater => "falling_dripstone_water",
                Liquid::DripLava => "falling_dripstone_lava",
                Liquid::Nectar => "falling_nectar",
                Liquid::Spore => "falling_spore_blossom",
            };
            queue(t, spawn(type_id(name), p.pos, p.vel, Data::None));
        }
        return false;
    }
    p.vel.y -= f64::from(p.gravity);
    let d = p.vel;
    mv(p, t.world, t.scratch, d);
    if hanging {
        p.vel *= 0.02;
    } else if matches!(p.kind, Kind::Fall(_)) && p.on_ground {
        let land = match l {
            Liquid::Water | Liquid::DripWater => Some("splash"),
            Liquid::Lava | Liquid::DripLava => Some("landing_lava"),
            Liquid::Honey => Some("landing_honey"),
            Liquid::Obsidian => Some("landing_obsidian_tear"),
            Liquid::Nectar | Liquid::Spore => None,
        };
        if let Some(name) = land {
            queue(t, spawn(type_id(name), p.pos, DVec3::ZERO, Data::None));
        }
        return false;
    }
    drag(p, 0.98);
    let fluid = match l {
        Liquid::Water | Liquid::DripWater => Some(Fluid::Water),
        Liquid::Lava | Liquid::DripLava => Some(Fluid::Lava),
        _ => None,
    };
    if let Some(fluid) = fluid {
        let (x, y, z) = cells(p.pos);
        if let Some((f, h)) = t.world.fluid(x, y, z)
            && f == fluid
            && p.pos.y < f64::from(y) + f64::from(h)
        {
            return false;
        }
    }
    true
}

/// Size multiplier over the particle's life (vanilla `getQuadSize`).
pub(crate) fn quad_size(p: &P, partial: f32) -> f32 {
    let a = (p.age as f32 + partial) / p.life.max(1) as f32;
    match p.kind {
        Kind::Ash(_)
        | Kind::FallingDust
        | Kind::DragonBreath
        | Kind::TrialDetect
        | Kind::Crit
        | Kind::Heart
        | Kind::Note
        | Kind::Dust
        | Kind::Transition
        | Kind::Cloud => p.size * (a * 32.0).clamp(0.0, 1.0),
        Kind::Flame => p.size * (1.0 - a * a * 0.5),
        Kind::LavaPop => p.size * (1.0 - a * a),
        Kind::Portal => {
            let s = 1.0 - a;
            p.size * (1.0 - s * s)
        }
        Kind::ReversePortal => p.size * (1.0 - a / 1.5),
        Kind::Shriek => p.size * (a * 0.75).clamp(0.0, 1.0),
        Kind::Flash => 7.1 * ((p.age as f32 + partial - 1.0) * 0.25 * std::f32::consts::PI).sin(),
        Kind::Animated if p.aux[4] > 0.5 && !twinkle_visible(p) => 0.0,
        k if k.invisible() => 0.0,
        _ => p.size,
    }
}

/// Vanilla's twinkling sparks skip every other three-tick window after the first third of life.
fn twinkle_visible(p: &P) -> bool {
    p.age < p.life / 3 || (p.age + p.life) / 3 % 2 == 0
}

pub(crate) fn alpha(p: &P, partial: f32) -> f32 {
    let t = (p.age as f32 + partial) / p.life.max(1) as f32;
    match p.kind {
        Kind::Flash => 0.6 - (p.age as f32 + partial - 1.0) * 0.25 * 0.5,
        Kind::VaultConnection => 0.6 * ((t - 0.25) / 0.75).clamp(0.0, 1.0),
        Kind::Shriek => 1.0 - t.clamp(0.0, 1.0),
        _ => p.color[3],
    }
}

pub(crate) fn emission(p: &P, partial: f32) -> f32 {
    let a = ((p.age as f32 + partial) / p.life.max(1) as f32).clamp(0.0, 1.0);
    if matches!(
        p.kind,
        Kind::Portal | Kind::ReversePortal | Kind::FlyTowards
    ) {
        a * a * a * a
    } else {
        a
    }
}
