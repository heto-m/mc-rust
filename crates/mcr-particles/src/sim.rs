use glam::DVec3;

use crate::atlas::{ParticleAtlas, Uv};
use crate::names::PARTICLE_NAMES;
use crate::rng::Rng;
use crate::{Fluid, ParticleWorld};

pub const POOL_CAP: usize = 16_384;
pub(crate) const MAX_SPAWNS_PER_TICK: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pool {
    Opaque = 0,
    Translucent = 1,
    Terrain = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Liquid {
    Water,
    Lava,
    Honey,
    Obsidian,
    Nectar,
    DripWater,
    DripLava,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Generic,
    Smoke,
    Flame,
    LavaPop,
    Explosion,
    Seed,
    Poof,
    Crit,
    Heart,
    Note,
    Portal,
    Dust,
    Transition,
    WaterDrop,
    Splash,
    Bubble,
    BubblePop,
    Hang(Liquid),
    Fall(Liquid),
    Land(Liquid),
    Cloud,
    Campfire,
    Sweep,
    Animated,
    Flash,
    Spell,
    Town,
    Terrain,
    Skip,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Light {
    World,
    Full,
    BlockMax,
    Emission,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Data {
    None,
    Dust { color: u32, scale: f32 },
    Transition { from: u32, to: u32, scale: f32 },
    Argb(u32),
    Rect { rect: Uv, tint: [f32; 3] },
    Firework(Spark),
}

/// A coloured firework spark; `trailed` spawns it half-aged like vanilla's trail copies.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spark {
    pub color: u32,
    pub fade: Option<u32>,
    pub trail: bool,
    pub twinkle: bool,
    pub trailed: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Spawn {
    pub type_id: u16,
    pub pos: DVec3,
    pub aux: DVec3,
    pub data: Data,
}

#[derive(Debug, Clone)]
pub(crate) struct P {
    pub pos: DVec3,
    pub prev: DVec3,
    pub start: DVec3,
    pub vel: DVec3,
    pub age: i32,
    pub life: i32,
    pub size: f32,
    pub color: [f32; 4],
    pub gravity: f32,
    pub friction: f32,
    pub roll: f32,
    pub prev_roll: f32,
    pub kind: Kind,
    pub light_mode: Light,
    pub light: (u8, u8),
    pub type_id: u16,
    pub sprite: u16,
    pub half_w: f32,
    pub height: f32,
    pub physics: bool,
    pub speed_up: bool,
    pub on_ground: bool,
    pub stopped: bool,
    pub rect: Uv,
    pub sub: [f32; 2],
    pub aux: [f32; 6],
}

pub(crate) fn type_id(name: &str) -> u16 {
    PARTICLE_NAMES
        .iter()
        .position(|n| *n == name)
        .map_or(0, |i| i as u16)
}

pub(crate) fn kind_of(name: &str) -> (Kind, Pool) {
    use Kind::*;
    use Liquid::*;
    let opaque = |k| (k, Pool::Opaque);
    let trans = |k| (k, Pool::Translucent);
    match name {
        "smoke" | "large_smoke" | "white_smoke" => opaque(Smoke),
        "flame" | "small_flame" | "soul_fire_flame" | "copper_fire_flame" => opaque(Flame),
        "lava" => opaque(LavaPop),
        "explosion" => opaque(Explosion),
        "explosion_emitter" => opaque(Seed),
        "poof" => opaque(Poof),
        "crit" | "damage_indicator" | "enchanted_hit" => opaque(Crit),
        "heart" | "angry_villager" => opaque(Heart),
        "happy_villager" | "composter" | "egg_crack" => opaque(Town),
        "note" => opaque(Note),
        "portal" => opaque(Portal),
        "dust" => opaque(Dust),
        "dust_color_transition" => opaque(Transition),
        "block" | "block_crumble" | "item" | "item_slime" | "item_cobweb" | "item_snowball" => {
            (Terrain, Pool::Terrain)
        }
        "rain" => opaque(WaterDrop),
        "splash" => opaque(Splash),
        "bubble" | "bubble_column_up" | "current_down" | "underwater" => opaque(Bubble),
        "bubble_pop" => opaque(BubblePop),
        "dripping_water" => opaque(Hang(Water)),
        "falling_water" => opaque(Fall(Water)),
        "dripping_lava" => opaque(Hang(Lava)),
        "falling_lava" => opaque(Fall(Lava)),
        "landing_lava" => opaque(Land(Lava)),
        "dripping_honey" => opaque(Hang(Honey)),
        "falling_honey" => opaque(Fall(Honey)),
        "landing_honey" => opaque(Land(Honey)),
        "dripping_obsidian_tear" => opaque(Hang(Obsidian)),
        "falling_obsidian_tear" => opaque(Fall(Obsidian)),
        "landing_obsidian_tear" => opaque(Land(Obsidian)),
        "falling_nectar" => opaque(Fall(Nectar)),
        "dripping_dripstone_water" => opaque(Hang(DripWater)),
        "falling_dripstone_water" => opaque(Fall(DripWater)),
        "dripping_dripstone_lava" => opaque(Hang(DripLava)),
        "falling_dripstone_lava" => opaque(Fall(DripLava)),
        "cloud" | "sneeze" => trans(Cloud),
        "campfire_cosy_smoke" | "campfire_signal_smoke" => trans(Campfire),
        "sweep_attack" => opaque(Sweep),
        "totem_of_undying" | "end_rod" | "firework" => trans(Animated),
        "witch" | "effect" | "instant_effect" | "entity_effect" => trans(Spell),
        "flash" => trans(Flash),
        "block_marker" | "elder_guardian" | "vibration" | "trail" | "shriek" => opaque(Skip),
        _ => opaque(Generic),
    }
}

fn base(pos: DVec3, rng: &mut Rng) -> P {
    P {
        pos,
        prev: pos,
        start: pos,
        vel: DVec3::ZERO,
        age: 0,
        life: (4.0 / (rng.f32() * 0.9 + 0.1)) as i32,
        size: 0.1 * (rng.f32() * 0.5 + 0.5) * 2.0,
        color: [1.0; 4],
        gravity: 0.0,
        friction: 0.98,
        roll: 0.0,
        prev_roll: 0.0,
        kind: Kind::Generic,
        light_mode: Light::World,
        light: (0, 15),
        type_id: 0,
        sprite: 0,
        half_w: 0.1,
        height: 0.2,
        physics: true,
        speed_up: false,
        on_ground: false,
        stopped: false,
        rect: Uv::ZERO,
        sub: [0.0; 2],
        aux: [0.0; 6],
    }
}

fn base_v(pos: DVec3, aux: DVec3, rng: &mut Rng) -> P {
    let mut p = base(pos, rng);
    let jitter = |rng: &mut Rng| f64::from(rng.f32() * 2.0 - 1.0) * 0.4;
    let mut v = DVec3::new(
        aux.x + jitter(rng),
        aux.y + jitter(rng),
        aux.z + jitter(rng),
    );
    let speed = f64::from((rng.f32() + rng.f32() + 1.0) * 0.15);
    let len = v.length();
    if len > 1e-9 {
        v = v / len * speed * 0.4;
    }
    v.y += 0.1;
    p.vel = v;
    p
}

fn set_size(p: &mut P, w: f32, h: f32) {
    p.half_w = w / 2.0;
    p.height = h;
}

fn scale(p: &mut P, s: f32) {
    p.size *= s;
    set_size(p, 0.2 * s, 0.2 * s);
}

fn rgb(c: u32) -> [f32; 3] {
    [
        ((c >> 16) & 255) as f32 / 255.0,
        ((c >> 8) & 255) as f32 / 255.0,
        (c & 255) as f32 / 255.0,
    ]
}

fn set_from_age(p: &mut P, atlas: &ParticleAtlas) {
    let set = atlas.set(usize::from(p.type_id));
    if let Some(last) = set.len().checked_sub(1) {
        let idx = if p.life > 0 {
            (p.age.max(0) as usize * last / p.life as usize).min(last)
        } else {
            0
        };
        p.sprite = set[idx];
    }
}

pub(crate) fn construct(s: &Spawn, kind: Kind, atlas: &ParticleAtlas, rng: &mut Rng) -> Option<P> {
    let (pos, aux) = (s.pos, s.aux);
    let set = atlas.set(usize::from(s.type_id));
    if set.is_empty() && !matches!(kind, Kind::Terrain | Kind::Seed) {
        return None;
    }
    let mut p = match kind {
        Kind::Skip => return None,
        Kind::Smoke => {
            let big = PARTICLE_NAMES[usize::from(s.type_id)] == "large_smoke";
            let sc = if big { 2.5 } else { 1.0 };
            let mut p = base_v(pos, DVec3::ZERO, rng);
            p.friction = 0.96;
            p.gravity = -0.1;
            p.speed_up = true;
            p.vel = p.vel * 0.1 + aux;
            let c = rng.f32() * 0.3;
            p.color = [c, c, c, 1.0];
            p.size *= 0.75 * sc;
            p.life = ((8.0 / (rng.f32() * 0.8 + 0.2) * sc) as i32).max(1);
            p
        }
        Kind::Flame => {
            let mut p = base_v(pos, aux, rng);
            p.friction = 0.96;
            p.vel = p.vel * 0.01 + aux;
            let jitter = |rng: &mut Rng| f64::from(rng.f32() - rng.f32()) * 0.05;
            p.pos += DVec3::new(jitter(rng), jitter(rng), jitter(rng));
            p.prev = p.pos;
            p.life = (8.0 / (rng.f32() as f64 * 0.8 + 0.2)) as i32 + 4;
            p.physics = false;
            p.light_mode = Light::Emission;
            if PARTICLE_NAMES[usize::from(s.type_id)] == "small_flame" {
                scale(&mut p, 0.5);
            }
            p
        }
        Kind::LavaPop => {
            let mut p = base_v(pos, DVec3::ZERO, rng);
            p.gravity = 0.75;
            p.friction = 0.999;
            p.vel *= 0.8;
            p.vel.y = f64::from(rng.f32() * 0.4 + 0.05);
            p.size *= rng.f32() * 2.0 + 0.2;
            p.life = (16.0 / (rng.f32() * 0.8 + 0.2)) as i32;
            p.light_mode = Light::BlockMax;
            p
        }
        Kind::Explosion => {
            let mut p = base(pos, rng);
            p.life = 6 + rng.below(4) as i32;
            let c = rng.f32() * 0.6 + 0.4;
            p.color = [c, c, c, 1.0];
            p.size = 2.0 * (1.0 - aux.x as f32 * 0.5);
            p.light_mode = Light::Full;
            p.physics = false;
            p
        }
        Kind::Seed => {
            let mut p = base(pos, rng);
            p.life = 8;
            p.physics = false;
            p
        }
        Kind::Poof => {
            let mut p = base(pos, rng);
            p.gravity = -0.1;
            p.friction = 0.9;
            let j = |rng: &mut Rng| f64::from(rng.f32() * 2.0 - 1.0) * 0.05;
            p.vel = DVec3::new(aux.x + j(rng), aux.y + j(rng), aux.z + j(rng));
            let c = rng.f32() * 0.3 + 0.7;
            p.color = [c, c, c, 1.0];
            p.size = 0.1 * (rng.f32() * rng.f32() * 6.0 + 1.0);
            p.life = (16.0 / (rng.f32() * 0.8 + 0.2)) as i32 + 2;
            p
        }
        Kind::Crit => {
            let name = PARTICLE_NAMES[usize::from(s.type_id)];
            let aux = if name == "damage_indicator" {
                DVec3::new(aux.x, aux.y + 1.0, aux.z)
            } else {
                aux
            };
            let mut p = base_v(pos, DVec3::ZERO, rng);
            p.friction = 0.7;
            p.gravity = 0.5;
            p.vel = p.vel * 0.1 + aux * 0.4;
            let c = rng.f32() * 0.3 + 0.6;
            p.color = [c, c, c, 1.0];
            if name == "enchanted_hit" {
                p.color[0] *= 0.3;
                p.color[1] *= 0.8;
            }
            p.size *= 0.75;
            p.life = ((6.0 / (rng.f32() as f64 * 0.8 + 0.6)) as i32).max(1);
            p.physics = false;
            p
        }
        Kind::Heart => {
            let angry = PARTICLE_NAMES[usize::from(s.type_id)] == "angry_villager";
            let mut p = base_v(
                if angry { pos + DVec3::Y * 0.5 } else { pos },
                DVec3::ZERO,
                rng,
            );
            p.speed_up = true;
            p.friction = 0.86;
            p.vel *= 0.01;
            p.vel.y += 0.1;
            p.size *= 1.5;
            p.life = 16;
            p.physics = false;
            p
        }
        Kind::Note => {
            let mut p = base_v(pos, DVec3::ZERO, rng);
            p.friction = 0.66;
            p.speed_up = true;
            p.vel *= 0.01;
            p.vel.y += 0.2;
            let c = aux.x as f32;
            let ch = |off: f32| {
                ((c + off) * std::f32::consts::TAU)
                    .sin()
                    .mul_add(0.65, 0.35)
                    .max(0.0)
            };
            p.color = [ch(0.0), ch(0.333_333_34), ch(0.666_666_7), 1.0];
            p.size *= 1.5;
            p.life = 6;
            p
        }
        Kind::Portal => {
            let mut p = base(pos, rng);
            p.vel = aux;
            p.size = 0.1 * (rng.f32() * 0.2 + 0.5);
            let br = rng.f32() * 0.6 + 0.4;
            p.color = [br * 0.9, br * 0.3, br, 1.0];
            p.life = (rng.f32() * 10.0) as i32 + 40;
            p.physics = false;
            p.light_mode = Light::Emission;
            p
        }
        Kind::Dust | Kind::Transition => {
            let (color, to, scale_v) = match s.data {
                Data::Dust { color, scale } => (color, color, scale),
                Data::Transition { from, to, scale } => (from, to, scale),
                _ => (0xff0000, 0xff0000, 1.0),
            };
            let sc = if scale_v.is_finite() {
                scale_v.clamp(0.01, 4.0)
            } else {
                1.0
            };
            let mut p = base_v(pos, aux, rng);
            p.friction = 0.96;
            p.speed_up = true;
            p.vel *= 0.1;
            p.size *= 0.75 * sc;
            let base_life = (8.0 / (rng.f64() * 0.8 + 0.2)) as i32;
            p.life = ((base_life as f32 * sc) as i32).max(1);
            let factor = rng.f32() * 0.4 + 0.6;
            let pick = |c: u32, rng: &mut Rng| {
                let [r, g, b] = rgb(c);
                [
                    (rng.f32() * 0.2 + 0.8) * r * factor,
                    (rng.f32() * 0.2 + 0.8) * g * factor,
                    (rng.f32() * 0.2 + 0.8) * b * factor,
                ]
            };
            let from = pick(color, rng);
            let to = pick(to, rng);
            p.color = [from[0], from[1], from[2], 1.0];
            p.aux = [to[0], to[1], to[2], from[0], from[1], from[2]];
            p
        }
        Kind::WaterDrop | Kind::Splash => {
            let mut p = base_v(pos, DVec3::ZERO, rng);
            p.vel.x *= 0.3;
            p.vel.y = f64::from(rng.f32() * 0.2 + 0.1);
            p.vel.z *= 0.3;
            set_size(&mut p, 0.01, 0.01);
            p.gravity = 0.06;
            p.life = (8.0 / (rng.f32() * 0.8 + 0.2)) as i32;
            if kind == Kind::Splash {
                p.gravity = 0.04;
                if aux.y == 0.0 && (aux.x != 0.0 || aux.z != 0.0) {
                    p.vel = DVec3::new(aux.x, 0.1, aux.z);
                }
            }
            p
        }
        Kind::Bubble => {
            let mut p = base(pos, rng);
            set_size(&mut p, 0.02, 0.02);
            p.size *= rng.f32() * 0.6 + 0.2;
            let j = |rng: &mut Rng| f64::from(rng.f32() * 2.0 - 1.0) * 0.02;
            p.vel = aux * 0.2 + DVec3::new(j(rng), j(rng), j(rng));
            p.life = (8.0 / (rng.f32() * 0.8 + 0.2)) as i32;
            p
        }
        Kind::BubblePop => {
            let mut p = base(pos, rng);
            p.life = 4;
            p.gravity = 0.008;
            p.vel = aux;
            p
        }
        Kind::Hang(l) | Kind::Fall(l) | Kind::Land(l) => {
            let mut p = base(pos, rng);
            set_size(&mut p, 0.01, 0.01);
            p.gravity = 0.06;
            let [r, g, b] = match l {
                Liquid::Water | Liquid::DripWater => [0.2, 0.3, 1.0],
                Liquid::Lava | Liquid::DripLava => [1.0, 0.285_714_3, 0.083_333_33],
                Liquid::Honey => [0.622_5, 0.507_5, 0.082_5],
                Liquid::Nectar => [0.92, 0.782, 0.72],
                Liquid::Obsidian => [0.51, 0.03, 0.67],
            };
            p.color = [r, g, b, 1.0];
            if matches!(kind, Kind::Hang(_)) {
                p.gravity *= 0.02;
                p.life = 40;
            }
            if matches!(kind, Kind::Land(_)) {
                p.life = (16.0 / (rng.f32() * 0.8 + 0.2)) as i32;
            }
            if matches!(l, Liquid::Lava | Liquid::DripLava) {
                p.light_mode = Light::BlockMax;
            }
            p
        }
        Kind::Cloud => {
            let mut p = base(pos, rng);
            p.friction = 0.96;
            p.vel = p.vel * 0.1 + aux;
            let c = 1.0 - rng.f32() * 0.3;
            p.color = [c, c, c, 1.0];
            p.size *= 1.875;
            let base_life = (8.0 / (rng.f32() as f64 * 0.8 + 0.3)) as i32;
            p.life = ((base_life as f32 * 2.5) as i32).max(1);
            p.physics = false;
            p
        }
        Kind::Campfire => {
            let signal = PARTICLE_NAMES[usize::from(s.type_id)] == "campfire_signal_smoke";
            let mut p = base(pos, rng);
            scale(&mut p, 3.0);
            set_size(&mut p, 0.25, 0.25);
            p.life = rng.below(50) as i32 + if signal { 280 } else { 80 };
            p.gravity = 3.0e-6;
            p.vel = DVec3::new(aux.x, aux.y + f64::from(rng.f32() / 500.0), aux.z);
            p
        }
        Kind::Sweep => {
            let mut p = base(pos, rng);
            p.life = 4;
            let c = rng.f32() * 0.6 + 0.4;
            p.color = [c, c, c, 1.0];
            p.size = 1.0 - aux.x as f32 * 0.5;
            p.light_mode = Light::Full;
            p.physics = false;
            p
        }
        Kind::Animated => {
            let name = PARTICLE_NAMES[usize::from(s.type_id)];
            let mut p = base(pos, rng);
            p.friction = 0.91;
            p.light_mode = Light::Full;
            p.vel = aux;
            p.size *= 0.75;
            p.life = 60 + rng.below(12) as i32;
            p.aux = [1.0, 1.0, 1.0, 0.0, 0.0, 0.0];
            match name {
                "totem_of_undying" => {
                    p.gravity = 1.25;
                    p.friction = 0.6;
                    p.color = if rng.below(4) == 0 {
                        [
                            0.6 + rng.f32() * 0.2,
                            0.6 + rng.f32() * 0.3,
                            rng.f32() * 0.2,
                            1.0,
                        ]
                    } else {
                        [
                            0.1 + rng.f32() * 0.2,
                            0.4 + rng.f32() * 0.3,
                            rng.f32() * 0.2,
                            1.0,
                        ]
                    };
                }
                "end_rod" => {
                    p.gravity = 0.0125;
                    p.physics = false;
                    p.aux = [0.949, 0.871, 0.788, 1.0, 0.0, 0.0];
                }
                _ => {
                    p.gravity = 0.1;
                    p.life = 48 + rng.below(12) as i32;
                    p.color = [1.0, 1.0, 1.0, 0.99];
                    if let Data::Firework(spark) = s.data {
                        let [r, g, b] = rgb(spark.color);
                        p.color = [r, g, b, 0.99];
                        let [fr, fg, fb] = rgb(spark.fade.unwrap_or(0));
                        let flag = |b: bool| if b { 1.0 } else { 0.0 };
                        p.aux = [
                            fr,
                            fg,
                            fb,
                            flag(spark.fade.is_some()),
                            flag(spark.twinkle),
                            flag(spark.trail),
                        ];
                        if spark.trailed {
                            p.age = p.life / 2;
                        }
                    } else {
                        p.aux = [0.0; 6];
                    }
                }
            }
            p
        }
        Kind::Spell => {
            let name = PARTICLE_NAMES[usize::from(s.type_id)];
            let mut p = base_v(
                pos,
                DVec3::new(0.5 - rng.f64(), aux.y, 0.5 - rng.f64()),
                rng,
            );
            p.friction = 0.96;
            p.gravity = -0.1;
            p.speed_up = true;
            p.vel.y *= 0.2;
            if aux.x == 0.0 && aux.z == 0.0 {
                p.vel.x *= 0.1;
                p.vel.z *= 0.1;
            }
            p.size *= 0.75;
            p.life = (8.0 / (rng.f32() * 0.8 + 0.2)) as i32;
            p.physics = false;
            p.color = match (name, s.data) {
                ("witch", _) => {
                    let f = rng.f32() * 0.5 + 0.35;
                    [f, 0.0, f, 1.0]
                }
                (_, Data::Argb(c)) => {
                    let [r, g, b] = rgb(c);
                    [r, g, b, ((c >> 24) & 255) as f32 / 255.0]
                }
                _ => [1.0; 4],
            };
            p.aux[3] = p.color[3];
            p
        }
        Kind::Town => {
            let mut p = base_v(pos, aux, rng);
            let br = rng.f32() * 0.1 + 0.2;
            p.color = [br, br, br, 1.0];
            set_size(&mut p, 0.02, 0.02);
            p.size *= rng.f32() * 0.6 + 0.5;
            p.vel *= 0.02;
            p.life = (20.0 / (rng.f32() * 0.8 + 0.2)) as i32;
            p.physics = false;
            if PARTICLE_NAMES[usize::from(s.type_id)] != "happy_villager" {
                p.color = [br, br, br, 1.0];
            } else {
                p.color = [1.0; 4];
            }
            p
        }
        Kind::Terrain => {
            let Data::Rect { rect, tint } = s.data else {
                return None;
            };
            let mut p = base_v(pos, aux, rng);
            p.gravity = 1.0;
            let shade = if PARTICLE_NAMES[usize::from(s.type_id)] == "block"
                || PARTICLE_NAMES[usize::from(s.type_id)] == "block_crumble"
            {
                0.6
            } else {
                1.0
            };
            p.color = [shade * tint[0], shade * tint[1], shade * tint[2], 1.0];
            p.size /= 2.0;
            p.rect = rect;
            p.sub = [rng.f32() * 3.0, rng.f32() * 3.0];
            p
        }
        Kind::Flash => {
            let mut p = base(pos, rng);
            p.life = 4;
            p.physics = false;
            p.light_mode = Light::Full;
            if let Data::Argb(c) = s.data {
                let [r, g, b] = rgb(c);
                p.color = [r, g, b, 1.0];
            }
            p
        }
        Kind::Generic => {
            let mut p = base_v(pos, aux, rng);
            p.friction = 0.96;
            p
        }
    };
    p.kind = kind;
    p.type_id = s.type_id;
    p.life = p.life.max(1);
    if !matches!(kind, Kind::Terrain | Kind::Seed) {
        p.sprite = set[0];
        set_from_age(&mut p, atlas);
    }
    Some(p)
}

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

/// Advances a particle one tick; returns false when it should be removed.
pub(crate) fn tick(p: &mut P, t: &mut Tick) -> bool {
    p.prev = p.pos;
    p.prev_roll = p.roll;
    let cell = |v: f64| v.floor().clamp(-3.0e7, 3.0e7) as i32;
    let at = (cell(p.pos.x), cell(p.pos.y), cell(p.pos.z));
    p.light = t.world.light(at.0, at.1, at.2);
    match p.kind {
        Kind::Seed => {
            for _ in 0..6 {
                let j = |r: &mut Rng| (r.f64() - r.f64()) * 4.0;
                let off = DVec3::new(j(t.rng), j(t.rng), j(t.rng));
                queue(
                    t,
                    Spawn {
                        type_id: type_id("explosion"),
                        pos: p.pos + off,
                        aux: DVec3::new(f64::from(p.age) / f64::from(p.life), 0.0, 0.0),
                        data: Data::None,
                    },
                );
            }
            p.age += 1;
            return p.age < p.life;
        }
        Kind::Portal => {
            p.age += 1;
            if p.age >= p.life {
                return false;
            }
            let a = p.age as f32 / p.life as f32;
            let mut s = a;
            s = -s + s * s * 2.0;
            s = 1.0 - s;
            let start = p.start;
            p.pos = DVec3::new(
                start.x + p.vel.x * f64::from(s),
                start.y + p.vel.y * f64::from(s) + f64::from(1.0 - a),
                start.z + p.vel.z * f64::from(s),
            );
            return true;
        }
        Kind::Bubble => {
            p.life -= 1;
            if p.life <= 0 {
                return false;
            }
            let name = crate::names::PARTICLE_NAMES[usize::from(p.type_id)];
            p.vel.y += if name == "current_down" {
                -0.002
            } else {
                0.002
            };
            let d = p.vel;
            mv(p, t.world, t.scratch, d);
            drag(p, 0.85);
            let (x, y, z) = (cell(p.pos.x), cell(p.pos.y), cell(p.pos.z));
            return matches!(t.world.fluid(x, y, z), Some((Fluid::Water, _)));
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
        Kind::WaterDrop | Kind::Splash => {
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
            let (x, y, z) = (cell(p.pos.x), cell(p.pos.y), cell(p.pos.z));
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
            return !(top > 0.0 && p.pos.y < f64::from(y) + f64::from(top));
        }
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
        Kind::Town => {
            p.life -= 1;
            if p.life <= 0 {
                return false;
            }
            p.pos += p.vel;
            drag(p, 0.99);
            return true;
        }
        Kind::Sweep | Kind::Explosion => {
            p.age += 1;
            if p.age >= p.life {
                return false;
            }
            set_from_age(p, t.atlas);
            return true;
        }
        _ => {}
    }

    p.age += 1;
    if p.age > p.life {
        return false;
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
        Kind::Smoke | Kind::Dust | Kind::Transition | Kind::Cloud | Kind::Generic | Kind::Poof => {
            set_from_age(p, t.atlas);
        }
        Kind::Crit => {
            p.color[1] *= 0.96;
            p.color[2] *= 0.9;
            set_from_age(p, t.atlas);
        }
        Kind::LavaPop => {
            let odds = p.age as f32 / p.life as f32;
            if t.rng.f32() > odds {
                queue(
                    t,
                    Spawn {
                        type_id: type_id("smoke"),
                        pos: p.pos,
                        aux: p.vel,
                        data: Data::None,
                    },
                );
            }
        }
        Kind::Animated | Kind::Spell => {
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
            if p.kind == Kind::Animated
                && p.aux[5] > 0.5
                && p.age < p.life / 2
                && (p.age + p.life) % 2 == 0
            {
                let color = p.color.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u32);
                let fade = p.aux.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u32);
                queue(
                    t,
                    Spawn {
                        type_id: p.type_id,
                        pos: p.pos,
                        aux: DVec3::ZERO,
                        data: Data::Firework(Spark {
                            color: color[0] << 16 | color[1] << 8 | color[2],
                            fade: (p.aux[3] > 0.5)
                                .then_some(fade[0] << 16 | fade[1] << 8 | fade[2]),
                            trail: false,
                            twinkle: p.aux[4] > 0.5,
                            trailed: true,
                        }),
                    },
                );
            }
        }
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

fn tick_drip(p: &mut P, t: &mut Tick, l: Liquid) -> bool {
    let hanging = matches!(p.kind, Kind::Hang(_));
    if hanging {
        if matches!(l, Liquid::Lava | Liquid::DripLava) {
            p.color[0] = 1.0;
            p.color[1] = 16.0 / (40 - p.life + 16) as f32;
            p.color[2] = 4.0 / (40 - p.life + 8) as f32;
        }
        p.life -= 1;
        if p.life <= 0 {
            let name = match l {
                Liquid::Water => "falling_water",
                Liquid::Lava => "falling_lava",
                Liquid::Honey => "falling_honey",
                Liquid::Obsidian => "falling_obsidian_tear",
                Liquid::DripWater => "falling_dripstone_water",
                Liquid::DripLava => "falling_dripstone_lava",
                Liquid::Nectar => "falling_nectar",
            };
            queue(
                t,
                Spawn {
                    type_id: type_id(name),
                    pos: p.pos,
                    aux: p.vel,
                    data: Data::None,
                },
            );
            return false;
        }
    } else {
        p.life -= 1;
        if p.life <= 0 {
            return false;
        }
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
            Liquid::Nectar => None,
        };
        if let Some(name) = land {
            queue(
                t,
                Spawn {
                    type_id: type_id(name),
                    pos: p.pos,
                    aux: DVec3::ZERO,
                    data: Data::None,
                },
            );
        }
        return false;
    }
    drag(p, 0.98);
    true
}

/// Size multiplier over the particle's life (vanilla `getQuadSize`).
pub(crate) fn quad_size(p: &P, partial: f32) -> f32 {
    let a = (p.age as f32 + partial) / p.life.max(1) as f32;
    match p.kind {
        Kind::Smoke
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
        Kind::Seed => 0.0,
        Kind::Flash => 7.1 * ((p.age as f32 + partial - 1.0) * 0.25 * std::f32::consts::PI).sin(),
        Kind::Animated if p.aux[4] > 0.5 && !twinkle_visible(p) => 0.0,
        _ => p.size,
    }
}

/// Vanilla's twinkling sparks skip every other three-tick window after the first third of life.
fn twinkle_visible(p: &P) -> bool {
    p.age < p.life / 3 || (p.age + p.life) / 3 % 2 == 0
}

pub(crate) fn alpha(p: &P, partial: f32) -> f32 {
    if p.kind == Kind::Flash {
        0.6 - (p.age as f32 + partial - 1.0) * 0.25 * 0.5
    } else {
        p.color[3]
    }
}

pub(crate) fn emission(p: &P, partial: f32) -> f32 {
    let a = ((p.age as f32 + partial) / p.life.max(1) as f32).clamp(0.0, 1.0);
    if p.kind == Kind::Portal {
        a * a * a * a
    } else {
        a
    }
}
