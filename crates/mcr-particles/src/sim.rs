use glam::DVec3;

use crate::atlas::{ParticleAtlas, Uv};
use crate::names::PARTICLE_NAMES;
use crate::rng::Rng;

mod step;

pub(crate) use step::{Scratch, Tick, alpha, emission, fade, quad_size, tick};

pub const POOL_CAP: usize = 16_384;
pub(crate) const MAX_SPAWNS_PER_TICK: usize = 4096;
/// Vanilla's `ParticleLimit.SPORE_BLOSSOM`.
pub(crate) const SPORE_LIMIT: usize = 1000;
const MAX_DATA_TICKS: i32 = 1200;
const MAX_WATER_BLOCKS: i32 = 16;

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
    Spore,
    DripWater,
    DripLava,
}

/// `BaseAshSmokeParticle` subclasses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Haze {
    Smoke,
    Large,
    White,
    Ash,
    WhiteAsh,
    Plume,
    Noxious,
    Geyser,
}

/// Invisible particles that spawn others over their life.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Emitter {
    GustLarge,
    GustSmall,
    Geyser,
    Noxious,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Town {
    Plain,
    Happy,
    Composter,
    Egg,
    Dolphin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Suspended {
    Underwater,
    Crimson,
    Warped,
    SporeAir,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Shine {
    Glow,
    WaxOn,
    WaxOff,
    Spark,
    Scrape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Leaves {
    Cherry,
    Pale,
    Tinted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Generic,
    Ash(Haze),
    Flame,
    LavaPop,
    Explosion,
    SonicBoom,
    Gust,
    Seed,
    Emitter(Emitter),
    Poof,
    Crit,
    Heart,
    Note,
    Portal,
    ReversePortal,
    Dust,
    Transition,
    WaterDrop,
    Splash,
    Bubble,
    BubbleColumn,
    CurrentDown,
    BubblePop,
    SulfurBubble,
    Hang(Liquid),
    Fall(Liquid),
    Land(Liquid),
    Cloud,
    Campfire,
    Sweep,
    Animated,
    SquidInk,
    Flash,
    Spell,
    Town(Town),
    Suspended(Suspended),
    Terrain,
    Goo,
    Marker,
    FlyTowards,
    VaultConnection,
    FlyStraight,
    Rising,
    Glow(Shine),
    Leaves(Leaves),
    FallingDust,
    DragonBreath,
    Sculk,
    Wake,
    Vertical,
    Snowflake,
    TrialDetect,
    Firefly,
    Shriek,
    Vibration,
    Trail,
    GeyserPlume,
    ElderGuardian,
}

impl Kind {
    /// Kinds that never draw a quad.
    pub(crate) fn invisible(self) -> bool {
        matches!(self, Kind::Seed | Kind::Emitter(_) | Kind::ElderGuardian)
    }

    fn needs_sprites(self) -> bool {
        !matches!(self, Kind::Terrain | Kind::Marker) && !self.invisible()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Light {
    World,
    Full,
    BlockMax,
    Emission,
    Firefly,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Target {
    Block([i32; 3]),
    Entity { id: i32, y_offset: f32 },
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Data {
    None,
    Dust {
        color: u32,
        scale: f32,
    },
    Transition {
        from: u32,
        to: u32,
        scale: f32,
    },
    Argb(u32),
    Spell {
        argb: u32,
        power: f32,
    },
    Power(f32),
    Roll(f32),
    Delay(i32),
    Geyser {
        water_blocks: i32,
        impulse: f32,
    },
    Vibration {
        target: Target,
        ticks: i32,
    },
    Trail {
        target: DVec3,
        color: u32,
        ticks: i32,
    },
    Rect {
        rect: Uv,
        tint: [f32; 3],
    },
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
    /// Per-kind state: a target position, emitter velocity or plume heights.
    pub extra: DVec3,
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
    pub flag: bool,
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

/// Vanilla `ParticleType#getOverrideLimiter`: shown at any distance and particle setting.
pub(crate) fn overrides_limiter(name: &str) -> bool {
    matches!(
        name,
        "block_marker"
            | "geyser"
            | "geyser_base"
            | "geyser_poof"
            | "geyser_plume"
            | "damage_indicator"
            | "elder_guardian"
            | "explosion_emitter"
            | "explosion"
            | "gust"
            | "gust_emitter_large"
            | "gust_emitter_small"
            | "sonic_boom"
            | "sculk_charge"
            | "sculk_charge_pop"
            | "vibration"
            | "poof"
            | "spit"
            | "squid_ink"
            | "sweep_attack"
            | "campfire_cosy_smoke"
            | "campfire_signal_smoke"
            | "glow_squid_ink"
            | "glow"
            | "wax_on"
            | "wax_off"
            | "electric_spark"
            | "scrape"
            | "trial_spawner_detection"
            | "trial_spawner_detection_ominous"
            | "vault_connection"
            | "ominous_spawning"
    )
}

pub(crate) fn kind_of(name: &str) -> (Kind, Pool) {
    use Kind::*;
    use Liquid::*;
    let opaque = |k| (k, Pool::Opaque);
    let trans = |k| (k, Pool::Translucent);
    match name {
        "smoke" => opaque(Ash(self::Haze::Smoke)),
        "large_smoke" => opaque(Ash(self::Haze::Large)),
        "white_smoke" => opaque(Ash(self::Haze::White)),
        "ash" => opaque(Ash(self::Haze::Ash)),
        "white_ash" => opaque(Ash(self::Haze::WhiteAsh)),
        "dust_plume" => opaque(Ash(self::Haze::Plume)),
        "noxious_gas" => trans(Ash(self::Haze::Noxious)),
        "geyser_base" | "geyser_poof" => opaque(Ash(self::Haze::Geyser)),
        "geyser_plume" => opaque(GeyserPlume),
        "geyser" => opaque(Emitter(self::Emitter::Geyser)),
        "noxious_gas_cloud" => opaque(Emitter(self::Emitter::Noxious)),
        "gust_emitter_large" => opaque(Emitter(self::Emitter::GustLarge)),
        "gust_emitter_small" => opaque(Emitter(self::Emitter::GustSmall)),
        "gust" | "small_gust" => opaque(Gust),
        "sonic_boom" => opaque(SonicBoom),
        "flame" | "small_flame" | "soul_fire_flame" | "copper_fire_flame" => opaque(Flame),
        "soul" | "sculk_soul" => trans(Rising),
        "lava" => opaque(LavaPop),
        "explosion" => opaque(Explosion),
        "explosion_emitter" => opaque(Seed),
        "elder_guardian" => opaque(ElderGuardian),
        "poof" | "spit" => opaque(Poof),
        "crit" | "damage_indicator" | "enchanted_hit" => opaque(Crit),
        "heart" | "angry_villager" => opaque(Heart),
        "happy_villager" => opaque(Town(self::Town::Happy)),
        "composter" => opaque(Town(self::Town::Composter)),
        "egg_crack" => opaque(Town(self::Town::Egg)),
        "mycelium" => opaque(Town(self::Town::Plain)),
        "dolphin" => opaque(Town(self::Town::Dolphin)),
        "underwater" => opaque(Suspended(self::Suspended::Underwater)),
        "crimson_spore" => opaque(Suspended(self::Suspended::Crimson)),
        "warped_spore" => opaque(Suspended(self::Suspended::Warped)),
        "spore_blossom_air" => opaque(Suspended(self::Suspended::SporeAir)),
        "glow" => opaque(Glow(Shine::Glow)),
        "wax_on" => opaque(Glow(Shine::WaxOn)),
        "wax_off" => opaque(Glow(Shine::WaxOff)),
        "electric_spark" => opaque(Glow(Shine::Spark)),
        "scrape" => opaque(Glow(Shine::Scrape)),
        "cherry_leaves" => opaque(Leaves(self::Leaves::Cherry)),
        "pale_oak_leaves"
        | "red_poplar_leaves"
        | "orange_poplar_leaves"
        | "yellow_poplar_leaves" => opaque(Leaves(self::Leaves::Pale)),
        "tinted_leaves" => opaque(Leaves(self::Leaves::Tinted)),
        "falling_dust" => opaque(FallingDust),
        "dragon_breath" => opaque(DragonBreath),
        "sculk_charge" | "sculk_charge_pop" => trans(Sculk),
        "fishing" => opaque(Wake),
        "pause_mob_growth" | "reset_mob_growth" => opaque(Vertical),
        "snowflake" => opaque(Snowflake),
        "trial_spawner_detection" | "trial_spawner_detection_ominous" => opaque(TrialDetect),
        "ominous_spawning" => opaque(FlyStraight),
        "firefly" => trans(Firefly),
        "shriek" => trans(Shriek),
        "vibration" => trans(Vibration),
        "trail" => opaque(Trail),
        "squid_ink" | "glow_squid_ink" => trans(SquidInk),
        "note" => opaque(Note),
        "portal" => opaque(Portal),
        "reverse_portal" => opaque(ReversePortal),
        "dust" => opaque(Dust),
        "dust_color_transition" => opaque(Transition),
        "block" | "block_crumble" | "dust_pillar" | "item" | "item_slime" | "item_cobweb"
        | "item_snowball" => (Terrain, Pool::Terrain),
        "sulfur_cube_goo" => opaque(Goo),
        "rain" => opaque(WaterDrop),
        "splash" => opaque(Splash),
        "bubble" => opaque(Bubble),
        "bubble_column_up" => opaque(BubbleColumn),
        "current_down" => opaque(CurrentDown),
        "sulfur_bubbles" => opaque(SulfurBubble),
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
        "falling_spore_blossom" => opaque(Fall(Spore)),
        "dripping_dripstone_water" => opaque(Hang(DripWater)),
        "falling_dripstone_water" => opaque(Fall(DripWater)),
        "dripping_dripstone_lava" => opaque(Hang(DripLava)),
        "falling_dripstone_lava" => opaque(Fall(DripLava)),
        "enchant" | "nautilus" => opaque(FlyTowards),
        "vault_connection" => trans(VaultConnection),
        "cloud" | "sneeze" => trans(Cloud),
        "campfire_cosy_smoke" | "campfire_signal_smoke" => trans(Campfire),
        "sweep_attack" => opaque(Sweep),
        "totem_of_undying" | "end_rod" | "firework" => trans(Animated),
        "witch" | "effect" | "instant_effect" | "entity_effect" | "infested" | "raid_omen"
        | "trial_omen" => trans(Spell),
        "flash" => trans(Flash),
        "block_marker" => (Marker, Pool::Terrain),
        _ => opaque(Generic),
    }
}

fn base(pos: DVec3, rng: &mut Rng) -> P {
    P {
        pos,
        prev: pos,
        start: pos,
        vel: DVec3::ZERO,
        extra: DVec3::ZERO,
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
        flag: false,
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

/// Vanilla `Particle#setPower`.
fn power(p: &mut P, power: f32) {
    if !power.is_finite() {
        return;
    }
    let k = f64::from(power.clamp(0.0, 16.0));
    p.vel.x *= k;
    p.vel.y = (p.vel.y - 0.1) * k + 0.1;
    p.vel.z *= k;
}

pub(crate) fn rgb(c: u32) -> [f32; 3] {
    [
        ((c >> 16) & 255) as f32 / 255.0,
        ((c >> 8) & 255) as f32 / 255.0,
        (c & 255) as f32 / 255.0,
    ]
}

fn color(p: &mut P, [r, g, b]: [f32; 3]) {
    p.color[0] = r;
    p.color[1] = g;
    p.color[2] = b;
}

pub(crate) fn set_from_age(p: &mut P, atlas: &ParticleAtlas) {
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

/// Kinds whose provider picks a random sprite instead of animating by age.
fn random_sprite(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::FlyTowards
            | Kind::VaultConnection
            | Kind::FlyStraight
            | Kind::Bubble
            | Kind::BubbleColumn
            | Kind::CurrentDown
            | Kind::SulfurBubble
            | Kind::Town(_)
            | Kind::Suspended(_)
            | Kind::Leaves(_)
            | Kind::Vertical
            | Kind::Firefly
            | Kind::Shriek
            | Kind::Vibration
            | Kind::Trail
            | Kind::ReversePortal
            | Kind::Hang(_)
            | Kind::Fall(_)
            | Kind::Land(_)
    )
}

fn data_ticks(t: i32) -> i32 {
    t.clamp(0, MAX_DATA_TICKS)
}

pub(crate) fn construct(s: &Spawn, kind: Kind, atlas: &ParticleAtlas, rng: &mut Rng) -> Option<P> {
    let (pos, aux) = (s.pos, s.aux);
    let set = atlas.set(usize::from(s.type_id));
    if set.is_empty() && kind.needs_sprites() {
        return None;
    }
    let name = PARTICLE_NAMES
        .get(usize::from(s.type_id))
        .copied()
        .unwrap_or("");
    let mut p = match kind {
        Kind::Ash(style) => ash(s, style, rng),
        Kind::Flame => {
            let mut p = rising(pos, aux, rng);
            p.physics = false;
            p.light_mode = Light::Emission;
            if name == "small_flame" {
                scale(&mut p, 0.5);
            }
            p
        }
        Kind::Rising => {
            let mut p = rising(pos, aux, rng);
            scale(&mut p, 1.5);
            if name == "sculk_soul" {
                p.light_mode = Light::BlockMax;
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
        Kind::Explosion | Kind::SonicBoom => {
            let mut p = base(pos, rng);
            p.life = 6 + rng.below(4) as i32;
            let c = rng.f32() * 0.6 + 0.4;
            p.color = [c, c, c, 1.0];
            p.size = 2.0 * (1.0 - aux.x as f32 * 0.5);
            if kind == Kind::SonicBoom {
                p.life = 16;
                p.size = 1.5;
            }
            p.light_mode = Light::Full;
            p.physics = false;
            p
        }
        Kind::Gust => {
            let mut p = base(pos, rng);
            p.life = 12 + rng.below(4) as i32;
            p.size = 1.0;
            set_size(&mut p, 1.0, 1.0);
            if name == "small_gust" {
                scale(&mut p, 0.15);
            }
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
        Kind::Emitter(e) => {
            let mut p = base(pos, rng);
            p.physics = false;
            p.extra = aux;
            p.life = match e {
                Emitter::GustLarge => 7,
                Emitter::GustSmall => 3,
                Emitter::Geyser | Emitter::Noxious => 20,
            };
            if let Data::Geyser { water_blocks, .. } = s.data {
                p.aux[0] = water_blocks.clamp(0, MAX_WATER_BLOCKS) as f32;
            }
            p
        }
        Kind::ElderGuardian => return None,
        Kind::Poof => {
            let mut p = base(pos, rng);
            p.gravity = if name == "spit" { 0.5 } else { -0.1 };
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
            let angry = name == "angry_villager";
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
        Kind::Portal | Kind::ReversePortal => {
            let mut p = base(pos, rng);
            p.vel = aux;
            p.size = 0.1 * (rng.f32() * 0.2 + 0.5);
            let br = rng.f32() * 0.6 + 0.4;
            p.color = [br * 0.9, br * 0.3, br, 1.0];
            p.life = (rng.f32() * 10.0) as i32 + 40;
            p.physics = false;
            p.light_mode = Light::Emission;
            if kind == Kind::ReversePortal {
                p.size *= 1.5;
                p.life = (rng.f32() * 2.0) as i32 + 60;
            }
            p
        }
        Kind::FlyTowards | Kind::VaultConnection => {
            let mut p = base(pos, rng);
            p.vel = aux;
            p.pos = pos + aux;
            p.prev = p.pos;
            p.size = 0.1 * (rng.f32() * 0.5 + 0.2);
            let br = rng.f32() * 0.6 + 0.4;
            p.color = [br * 0.9, br * 0.9, br, 1.0];
            p.life = (rng.f32() * 10.0) as i32 + 30;
            p.physics = false;
            if kind == Kind::VaultConnection {
                p.size *= 1.5;
                p.light_mode = Light::BlockMax;
                p.color[3] = 0.0;
            } else {
                p.light_mode = Light::Emission;
            }
            p
        }
        Kind::FlyStraight => {
            let mut p = base(pos, rng);
            p.vel = aux;
            p.pos = pos + aux;
            p.prev = p.pos;
            p.size = 0.1 * (rng.f32() * 0.5 + 0.2);
            p.physics = false;
            p.life = (rng.f32() * 5.0) as i32 + 25;
            p.light_mode = Light::BlockMax;
            let [r, g, b] = rgb(OMINOUS_START);
            p.color = [r, g, b, 1.0];
            scale(&mut p, 3.0 + rng.f32() * 2.0);
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
        Kind::Bubble | Kind::BubbleColumn | Kind::SulfurBubble => {
            let mut p = base(pos, rng);
            set_size(&mut p, 0.02, 0.02);
            p.size *= rng.f32() * 0.6 + 0.2;
            let j = |rng: &mut Rng| f64::from(rng.f32() * 2.0 - 1.0) * 0.02;
            p.vel = aux * 0.2 + DVec3::new(j(rng), j(rng), j(rng));
            p.life = (8.0 / (rng.f32() * 0.8 + 0.2)) as i32;
            if kind == Kind::BubbleColumn {
                p.gravity = -0.125;
                p.friction = 0.85;
                p.life = (40.0 / (rng.f32() * 0.8 + 0.2)) as i32;
            } else if kind == Kind::SulfurBubble {
                p.gravity = -0.04;
                p.friction = 0.85;
                p.vel.y = 0.0;
                let start = 0.02 + 0.02 * rng.f32();
                p.size = start;
                p.aux[0] = start;
                p.life = i32::MAX;
                p.extra = DVec3::new(pos.y, pos.y + 3.0, pos.y);
            }
            p
        }
        Kind::CurrentDown => {
            let mut p = base(pos, rng);
            p.life = (rng.f32() * 60.0) as i32 + 30;
            p.physics = false;
            p.vel = DVec3::new(0.0, -0.05, 0.0);
            set_size(&mut p, 0.02, 0.02);
            p.size *= rng.f32() * 0.6 + 0.2;
            p
        }
        Kind::BubblePop => {
            let mut p = base(pos, rng);
            p.life = 4;
            p.gravity = 0.008;
            p.vel = aux;
            p
        }
        Kind::Hang(l) | Kind::Fall(l) | Kind::Land(l) => drip(pos, kind, l, rng),
        Kind::Cloud => {
            let mut p = base(pos, rng);
            p.friction = 0.96;
            p.vel = p.vel * 0.1 + aux;
            let c = 1.0 - rng.f32() * 0.3;
            p.color = [c, c, c, 1.0];
            if name == "sneeze" {
                p.color = [0.22, 1.0, 0.53, 0.4];
            }
            p.size *= 1.875;
            let base_life = (8.0 / (rng.f32() as f64 * 0.8 + 0.3)) as i32;
            p.life = ((base_life as f32 * 2.5) as i32).max(1);
            p.physics = false;
            p
        }
        Kind::Campfire => {
            let signal = name == "campfire_signal_smoke";
            let mut p = base(pos, rng);
            scale(&mut p, 3.0);
            set_size(&mut p, 0.25, 0.25);
            p.life = rng.below(50) as i32 + if signal { 280 } else { 80 };
            p.color[3] = if signal { 0.95 } else { 0.9 };
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
        Kind::Animated => animated(s, name, rng),
        Kind::SquidInk => {
            let mut p = base(pos, rng);
            p.friction = 0.92;
            p.size = 0.5;
            p.color = if name == "glow_squid_ink" {
                [0.2, 0.8, 0.6, 1.0]
            } else {
                [0.0, 0.0, 0.0, 1.0]
            };
            p.life = (0.5 * 12.0 / (rng.f32() * 0.8 + 0.2)) as i32;
            p.physics = false;
            p.vel = aux;
            p.light_mode = Light::Full;
            p
        }
        Kind::Spell => {
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
                (_, Data::Spell { argb, .. }) => {
                    let [r, g, b] = rgb(argb);
                    [r, g, b, 1.0]
                }
                _ => [1.0; 4],
            };
            if let Data::Spell { power: k, .. } = s.data {
                power(&mut p, k);
            }
            p.aux[3] = p.color[3];
            p
        }
        Kind::Town(t) => {
            let mut p = base_v(pos, aux, rng);
            let br = rng.f32() * 0.1 + 0.2;
            p.color = [br, br, br, 1.0];
            set_size(&mut p, 0.02, 0.02);
            p.size *= rng.f32() * 0.6 + 0.5;
            p.vel *= 0.02;
            p.life = (20.0 / (rng.f32() * 0.8 + 0.2)) as i32;
            p.physics = false;
            match t {
                Town::Plain => {}
                Town::Happy | Town::Egg => p.color = [1.0; 4],
                Town::Composter => {
                    p.color = [1.0; 4];
                    p.life = 3 + rng.below(5) as i32;
                }
                Town::Dolphin => {
                    p.color = [0.3, 0.5, 1.0, 1.0 - rng.f32() * 0.7];
                    p.life /= 2;
                }
            }
            p
        }
        Kind::Suspended(sus) => {
            let at = pos - DVec3::Y * 0.125;
            let mut p = match sus {
                Suspended::Underwater => {
                    let mut p = base(at, rng);
                    p.size *= rng.f32() * 0.6 + 0.2;
                    p
                }
                _ => {
                    let v = match sus {
                        Suspended::Crimson => DVec3::new(
                            rng.gaussian() * 1.0e-6,
                            rng.gaussian() * 1.0e-4,
                            rng.gaussian() * 1.0e-6,
                        ),
                        Suspended::Warped => {
                            DVec3::new(0.0, rng.f64() * -1.9 * rng.f64() * 0.1, 0.0)
                        }
                        _ => DVec3::new(0.0, -0.8, 0.0),
                    };
                    let mut p = base_v(at, v, rng);
                    p.size *= rng.f32() * 0.6 + 0.6;
                    p
                }
            };
            set_size(&mut p, 0.01, 0.01);
            p.life = (16.0 / (rng.f32() * 0.8 + 0.2)) as i32;
            p.physics = false;
            p.friction = 1.0;
            match sus {
                Suspended::Underwater => color(&mut p, [0.4, 0.4, 0.7]),
                Suspended::Crimson => color(&mut p, [0.9, 0.4, 0.5]),
                Suspended::Warped => {
                    color(&mut p, [0.1, 0.1, 0.3]);
                    set_size(&mut p, 0.001, 0.001);
                }
                Suspended::SporeAir => {
                    p.life = 500 + rng.below(501) as i32;
                    p.gravity = 0.01;
                    color(&mut p, [0.32, 0.5, 0.22]);
                }
            }
            p
        }
        Kind::Glow(g) => {
            let v0 = if g == Shine::Glow {
                DVec3::new(0.5 - rng.f64(), aux.y, 0.5 - rng.f64())
            } else {
                DVec3::ZERO
            };
            let mut p = base_v(pos, v0, rng);
            p.friction = 0.96;
            p.speed_up = true;
            p.size *= 0.75;
            p.physics = false;
            p.light_mode = Light::Emission;
            let wax = DVec3::new(aux.x * 0.005, aux.y * 0.01, aux.z * 0.005);
            match g {
                Shine::Spark => {
                    color(&mut p, [1.0, 0.9, 1.0]);
                    p.vel = aux * 0.25;
                    p.life = rng.below(2) as i32 + 2;
                }
                Shine::Glow => {
                    let c = if rng.bool() {
                        [0.6, 1.0, 0.8]
                    } else {
                        [0.08, 0.4, 0.4]
                    };
                    color(&mut p, c);
                    p.vel.y *= 0.2;
                    if aux.x == 0.0 && aux.z == 0.0 {
                        p.vel.x *= 0.1;
                        p.vel.z *= 0.1;
                    }
                    p.life = (8.0 / (rng.f64() * 0.8 + 0.2)) as i32;
                }
                Shine::Scrape => {
                    let c = if rng.bool() {
                        [0.29, 0.58, 0.51]
                    } else {
                        [0.43, 0.77, 0.62]
                    };
                    color(&mut p, c);
                    p.vel = aux * 0.01;
                    p.life = rng.below(30) as i32 + 10;
                }
                Shine::WaxOff => {
                    color(&mut p, [1.0, 0.9, 1.0]);
                    p.vel = wax;
                    p.life = rng.below(30) as i32 + 10;
                }
                Shine::WaxOn => {
                    color(&mut p, [0.91, 0.55, 0.08]);
                    p.vel = wax;
                    p.life = rng.below(30) as i32 + 10;
                }
            }
            p
        }
        Kind::Leaves(l) => leaves(s, l, rng),
        Kind::FallingDust => {
            let mut p = base(pos, rng);
            if let Data::Argb(c) = s.data {
                color(&mut p, rgb(c));
            }
            p.size *= 0.675;
            let base_life = (32.0 / (rng.f32() * 0.8 + 0.2)) as i32;
            p.life = ((base_life as f32 * 0.9) as i32).max(1);
            p.aux[0] = (rng.f32() - 0.5) * 0.1;
            p.roll = rng.f32() * std::f32::consts::TAU;
            p.prev_roll = p.roll;
            p
        }
        Kind::DragonBreath => {
            let mut p = base(pos, rng);
            p.friction = 0.96;
            p.vel = aux;
            p.color = [
                0.717_647_1 + rng.f32() * (0.874_509_8 - 0.717_647_1),
                0.0,
                0.823_529_4 + rng.f32() * (0.976_470_6 - 0.823_529_4),
                1.0,
            ];
            p.size *= 0.75;
            p.life = (20.0 / (rng.f32() * 0.8 + 0.2)) as i32;
            p.physics = false;
            if let Data::Power(k) = s.data {
                power(&mut p, k);
            }
            p
        }
        Kind::Sculk => {
            let mut p = base_v(pos, aux, rng);
            p.friction = 0.96;
            p.physics = false;
            p.vel = aux;
            p.light_mode = Light::BlockMax;
            if name == "sculk_charge_pop" {
                p.life = rng.below(4) as i32 + 6;
            } else {
                scale(&mut p, 1.5);
                p.life = rng.below(12) as i32 + 8;
                if let Data::Roll(r) = s.data
                    && r.is_finite()
                {
                    p.roll = r;
                    p.prev_roll = r;
                }
            }
            p
        }
        Kind::Wake => {
            let mut p = base_v(pos, DVec3::ZERO, rng);
            set_size(&mut p, 0.01, 0.01);
            p.life = (8.0 / (rng.f32() * 0.8 + 0.2)) as i32;
            p.vel = aux;
            p
        }
        Kind::Vertical => {
            let mut p = base_v(pos, aux, rng);
            p.vel = aux;
            p.vel.y += if name == "reset_mob_growth" {
                0.03
            } else {
                -0.03
            };
            p.size *= rng.f32() * 0.6 + 0.5;
            p.life = 8;
            p
        }
        Kind::Snowflake => {
            let mut p = base(pos, rng);
            p.gravity = 0.225;
            p.friction = 1.0;
            let j = |rng: &mut Rng| f64::from(rng.f32() * 2.0 - 1.0) * 0.05;
            p.vel = DVec3::new(aux.x + j(rng), aux.y + j(rng), aux.z + j(rng));
            p.size = 0.1 * (rng.f32() * rng.f32() + 1.0);
            p.life = (16.0 / (rng.f32() * 0.8 + 0.2)) as i32 + 2;
            p.color = [0.923, 0.964, 0.999, 1.0];
            p
        }
        Kind::TrialDetect => {
            let mut p = base_v(pos, DVec3::ZERO, rng);
            p.friction = 0.96;
            p.gravity = -0.1;
            p.speed_up = true;
            p.vel = DVec3::new(aux.x, p.vel.y * 0.9 + aux.y, aux.z);
            p.size *= 0.75 * 1.5;
            p.life = ((8.0 / (0.5 + rng.f32() * 0.5) * 1.5) as i32).max(1);
            p.light_mode = Light::BlockMax;
            p
        }
        Kind::Firefly => {
            let y = if rng.bool() { aux.y } else { -aux.y };
            let mut p = base_v(pos, DVec3::new(0.5 - rng.f64(), y, 0.5 - rng.f64()), rng);
            p.speed_up = true;
            p.friction = 0.96;
            p.size *= 0.75;
            p.vel *= 0.8;
            p.life = 200 + rng.below(101) as i32;
            scale(&mut p, 1.5);
            p.color[3] = 0.0;
            p.light_mode = Light::Firefly;
            p
        }
        Kind::Shriek => {
            let mut p = base_v(pos, DVec3::ZERO, rng);
            p.size = 0.85;
            p.life = 30;
            p.vel = DVec3::new(0.0, 0.1, 0.0);
            p.light_mode = Light::BlockMax;
            if let Data::Delay(d) = s.data {
                p.aux[0] = data_ticks(d) as f32;
            }
            p
        }
        Kind::Vibration => {
            let Data::Vibration { target, ticks } = s.data else {
                return None;
            };
            let mut p = base_v(pos, DVec3::ZERO, rng);
            p.size = 0.3;
            p.life = data_ticks(ticks);
            p.light_mode = Light::BlockMax;
            p.physics = false;
            match target {
                Target::Block([x, y, z]) => {
                    p.extra = DVec3::new(f64::from(x), f64::from(y), f64::from(z)) + 0.5;
                }
                Target::Entity { id, y_offset } => {
                    p.flag = true;
                    let dy = if y_offset.is_finite() {
                        y_offset.clamp(-64.0, 64.0)
                    } else {
                        0.0
                    };
                    p.extra = DVec3::new(f64::from(id), f64::from(dy), 0.0);
                }
            }
            p
        }
        Kind::Trail => {
            let Data::Trail {
                target,
                color: c,
                ticks,
            } = s.data
            else {
                return None;
            };
            let mut p = base_v(pos, aux, rng);
            let [r, g, b] = rgb(c);
            let k = |rng: &mut Rng| 0.875 + rng.f32() * 0.25;
            let ch = |v: f32, k: f32| ((v * 255.0 * k) as i32).clamp(0, 255) as f32 / 255.0;
            p.color = [ch(r, k(rng)), ch(g, k(rng)), ch(b, k(rng)), 1.0];
            p.size = 0.26;
            p.extra = target;
            p.life = data_ticks(ticks);
            p.light_mode = Light::Full;
            p.physics = false;
            p
        }
        Kind::GeyserPlume => plume(s, rng),
        Kind::Terrain => {
            let Data::Rect { rect, tint } = s.data else {
                return None;
            };
            let block = matches!(name, "block" | "block_crumble" | "dust_pillar");
            let mut p = base_v(pos, if block { aux } else { DVec3::ZERO }, rng);
            match name {
                "item" => p.vel = p.vel * 0.1 + aux,
                "block_crumble" => {
                    p.vel = DVec3::ZERO;
                    p.life = rng.below(10) as i32 + 1;
                }
                "dust_pillar" => {
                    p.vel = DVec3::new(
                        rng.gaussian() / 30.0,
                        aux.y + rng.gaussian() / 2.0,
                        rng.gaussian() / 30.0,
                    );
                    p.life = rng.below(20) as i32 + 20;
                }
                _ => {}
            }
            p.gravity = 1.0;
            let shade = if block { 0.6 } else { 1.0 };
            p.color = [shade * tint[0], shade * tint[1], shade * tint[2], 1.0];
            p.size /= 2.0;
            p.rect = rect;
            p.sub = [rng.f32() * 3.0, rng.f32() * 3.0];
            p
        }
        Kind::Goo => {
            let mut p = base_v(pos, DVec3::ZERO, rng);
            p.gravity = 1.0;
            p.size /= 2.0;
            p.rect = atlas.uv(set[0]);
            p.sub = [rng.f32() * 3.0, rng.f32() * 3.0];
            p
        }
        Kind::Marker => {
            let Data::Rect { rect, .. } = s.data else {
                return None;
            };
            let mut p = base(pos, rng);
            p.life = 80;
            p.size = 0.5;
            p.physics = false;
            p.rect = rect;
            p.sub = [0.0; 2];
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
    if !set.is_empty() && !matches!(kind, Kind::Terrain | Kind::Marker | Kind::Goo) {
        if random_sprite(kind) {
            p.sprite = set[rng.below(set.len() as u32) as usize];
        } else {
            p.sprite = set[0];
            set_from_age(&mut p, atlas);
        }
    }
    Some(p)
}

const OMINOUS_START: u32 = (-12_210_434i32) as u32;

/// Vanilla `RisingParticle`.
fn rising(pos: DVec3, aux: DVec3, rng: &mut Rng) -> P {
    let mut p = base_v(pos, aux, rng);
    p.friction = 0.96;
    p.vel = p.vel * 0.01 + aux;
    let jitter = |rng: &mut Rng| f64::from(rng.f32() - rng.f32()) * 0.05;
    p.pos += DVec3::new(jitter(rng), jitter(rng), jitter(rng));
    p.prev = p.pos;
    p.start = p.pos;
    p.life = (8.0 / (rng.f32() as f64 * 0.8 + 0.2)) as i32 + 4;
    p
}

/// Vanilla `BaseAshSmokeParticle` and its subclasses.
fn ash(s: &Spawn, style: Haze, rng: &mut Rng) -> P {
    let mut pos = s.pos;
    let mut aux = s.aux;
    let (mut dir, col_rand, max_life, gravity, physics, mut sc) = match style {
        Haze::Smoke | Haze::White => ([0.1; 3], 0.3, 8.0, -0.1, true, 1.0),
        Haze::Large => ([0.1; 3], 0.3, 8.0, -0.1, true, 2.5),
        Haze::Ash => ([0.1, -0.1, 0.1], 0.5, 20.0, 0.1, false, 1.0),
        Haze::WhiteAsh => ([0.1, -0.1, 0.1], 0.0, 20.0, 0.0125, false, 1.0),
        Haze::Plume => ([0.7, 0.6, 0.7], 0.5, 7.0, 0.5, false, 1.0),
        Haze::Noxious => ([0.1; 3], 0.3, 5.0, -0.02, true, 3.0),
        Haze::Geyser => ([1.0; 3], 0.0, 0.0, 0.0, true, 1.0),
    };
    match style {
        Haze::Ash => aux = DVec3::ZERO,
        Haze::WhiteAsh => {
            let mut f = |k: f64| rng.f64() * k * rng.f64() * 0.1;
            aux = DVec3::new(f(-1.9), f(-0.5) * 5.0, f(-1.9));
        }
        Haze::Plume => aux.y += 0.15,
        Haze::Geyser => {
            let (water, impulse) = match s.data {
                Data::Geyser {
                    water_blocks,
                    impulse,
                } if impulse.is_finite() => (
                    water_blocks.clamp(0, MAX_WATER_BLOCKS) as f32,
                    impulse.clamp(0.0, 8.0),
                ),
                _ => (0.0, 1.5),
            };
            let burst = impulse + 0.25 * water;
            dir = [burst; 3];
            sc = 3.0 + 0.125 * water;
            let mut j = |k: f64| f64::from(rng.f32() - 0.5) * k;
            pos += DVec3::new(j(0.5), j(0.5) + 0.2, j(0.5));
        }
        _ => {}
    }
    let mut p = base_v(pos, DVec3::ZERO, rng);
    p.friction = 0.96;
    p.gravity = gravity;
    p.speed_up = true;
    p.vel = p.vel * DVec3::from_array(dir.map(f64::from)) + aux;
    let c = rng.f32() * col_rand;
    p.color = [c, c, c, 1.0];
    p.size *= 0.75 * sc;
    p.life = ((max_life / (rng.f32() * 0.8 + 0.2) * sc) as i32).max(1);
    p.physics = physics;
    match style {
        Haze::White => color(&mut p, [0.729_411_8, 0.694_117_67, 0.760_784_3]),
        Haze::WhiteAsh => color(&mut p, rgb(0xBAB1C2)),
        Haze::Plume => {
            let shift = rng.f32() * 0.2;
            color(&mut p, rgb(0xBAB1C2).map(|c| c - shift));
        }
        Haze::Noxious => {
            color(&mut p, [1.0; 3]);
            p.life = ((6.0 / (rng.f64() * 0.5 + 0.5) * 3.0) as i32).max(1);
            p.aux[0] = p.life as f32 / 2.0;
        }
        Haze::Geyser => {
            p.friction = 0.725;
            color(&mut p, [1.0; 3]);
            p.vel.y = p.vel.y.abs();
            p.life = (25.0 * (0.8 + 0.2 * rng.f32())) as i32;
        }
        _ => {}
    }
    p
}

fn drip(pos: DVec3, kind: Kind, l: Liquid, rng: &mut Rng) -> P {
    let mut p = base(pos, rng);
    set_size(&mut p, 0.01, 0.01);
    p.gravity = 0.06;
    let [r, g, b] = match (kind, l) {
        (_, Liquid::Water | Liquid::DripWater) => [0.2, 0.3, 1.0],
        (_, Liquid::Lava | Liquid::DripLava) => [1.0, 0.285_714_3, 0.083_333_33],
        (Kind::Fall(_), Liquid::Honey) => [0.582, 0.448, 0.082],
        (Kind::Land(_), Liquid::Honey) => [0.522, 0.408, 0.082],
        (_, Liquid::Honey) => [0.622, 0.508, 0.082],
        (_, Liquid::Nectar) => [0.92, 0.782, 0.72],
        (_, Liquid::Spore) => [0.32, 0.5, 0.22],
        (_, Liquid::Obsidian) => [0.511_718_75, 0.031_25, 0.890_625],
    };
    p.color = [r, g, b, 1.0];
    let f = |rng: &mut Rng| rng.f32() * 0.8 + 0.2;
    match kind {
        Kind::Hang(_) => {
            p.gravity *= 0.02;
            p.life = if matches!(l, Liquid::Honey | Liquid::Obsidian) {
                100
            } else {
                40
            };
        }
        Kind::Fall(_) => {
            p.life = (64.0 / f(rng)) as i32;
            match l {
                Liquid::Nectar => {
                    p.life = (16.0 / f(rng)) as i32;
                    p.gravity = 0.007;
                }
                Liquid::Spore => {
                    p.life = (64.0 / (0.1 + rng.f32() * 0.8)) as i32;
                    p.gravity = 0.005;
                }
                Liquid::Honey | Liquid::Obsidian => p.gravity = 0.01,
                _ => {}
            }
        }
        _ => {
            p.life = match l {
                Liquid::Honey => (128.0 / f(rng)) as i32,
                Liquid::Obsidian => (28.0 / f(rng)) as i32,
                _ => (16.0 / f(rng)) as i32,
            };
        }
    }
    if matches!(l, Liquid::Lava | Liquid::DripLava | Liquid::Obsidian) {
        p.light_mode = Light::BlockMax;
    }
    p
}

fn animated(s: &Spawn, name: &str, rng: &mut Rng) -> P {
    let mut p = base(s.pos, rng);
    p.friction = 0.91;
    p.light_mode = Light::Full;
    p.vel = s.aux;
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

/// Vanilla `FallingParticle` behind every falling-leaves type.
fn leaves(s: &Spawn, l: Leaves, rng: &mut Rng) -> P {
    let (fall, side, swirl, flow, sc, start) = match l {
        Leaves::Cherry => (0.25f32, 2.0f32, false, true, 1.0f32, 0.0f32),
        Leaves::Pale | Leaves::Tinted => (0.07, 10.0, true, false, 2.0, 0.021),
    };
    let mut p = base(s.pos, rng);
    let deg = |d: f32| d.to_radians();
    let rot_speed = deg(if rng.bool() { -30.0 } else { 30.0 });
    let spin = deg(if rng.bool() { -5.0 } else { 5.0 });
    p.life = LEAVES_LIFE;
    p.gravity = fall * 1.2 * 0.0025;
    let size = sc * if rng.bool() { 0.05 } else { 0.075 };
    p.size = size;
    set_size(&mut p, size, size);
    p.friction = 1.0;
    p.vel = DVec3::new(0.0, -f64::from(start), 0.0);
    let r = rng.f32();
    let a = f64::from(r * 60.0).to_radians();
    p.aux = [
        rot_speed,
        spin,
        side,
        (a.cos() * f64::from(side)) as f32,
        (a.sin() * f64::from(side)) as f32,
        (1000.0 + f64::from(r) * 3000.0).to_radians() as f32,
    ];
    p.flag = swirl;
    p.extra.x = if flow { 1.0 } else { 0.0 };
    if let (Leaves::Tinted, Data::Argb(c)) = (l, s.data) {
        color(&mut p, rgb(c));
    }
    p
}

pub(crate) const LEAVES_LIFE: i32 = 300;

/// Vanilla `GeyserPlumeParticle`: a column of water that rises to its height, then fades.
fn plume(s: &Spawn, rng: &mut Rng) -> P {
    let water = match s.data {
        Data::Geyser { water_blocks, .. } => water_blocks.clamp(0, MAX_WATER_BLOCKS),
        _ => 0,
    };
    let mut j = |k: f32| f64::from((rng.f32() - 0.5) * k);
    let pos = s.pos + DVec3::new(j(0.2), 0.0, j(0.2)) + DVec3::Y * f64::from(rng.f32());
    let mut p = base_v(pos, s.aux, rng);
    let height = 5 * water.max(1);
    p.speed_up = true;
    p.life = height * 5;
    p.vel.y = 0.0;
    p.friction = 1.0;
    let propulsion = if water == 1 { 1.5 } else { 1.0 } * height as f32 * 1.45;
    p.gravity = -propulsion;
    let base_size = p.size * 0.75;
    let min = base_size * (2.0 + height as f32 / 8.0);
    let max = base_size * (3.0 + height as f32 / 8.0);
    p.size = min;
    p.extra = DVec3::new(
        pos.y,
        pos.y + f64::from(height) - 1.0,
        f64::from(propulsion),
    );
    p.aux = [
        (rng.f32() - 0.5) * 0.2,
        (rng.f32() - 0.5) * 0.2,
        min,
        max,
        0.0,
        0.0,
    ];
    p
}
