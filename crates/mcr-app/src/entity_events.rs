//! Client effects of `entity_event` packets: vanilla's per-class `handleEntityEvent` overrides.
//!
//! Sounds vanilla plays through `Entity.playSound` are silent on a client for non-local entities (the
//! server sends them), so only its `playLocalSound` calls appear here.

use mcr_audio::SoundSource;
use mcr_client::ClientState;
use mcr_entity_model::EventAnim;
use mcr_protocol::types::component::DataComponent;
use mcr_protocol::types::entity_data::EntityDataValue;
use mcr_protocol::types::item::ItemStackTemplate;
use mcr_protocol::types::particle::{BlockParticleKind, ColorParticleKind};
use mcr_protocol::types::{ItemStack, Particle};
use rustc_hash::FxHashMap;

use crate::audio::Audio;
use crate::effects::{WorldEffects, particle_id};
use crate::entity_class::class_of;

const MAX_TIMERS: usize = 4096;
const SILENT: u8 = 4;
const LIVING_FLAGS: u8 = 8;
const BABY: u8 = 16;
const ITEM_DATA: u8 = 8;
const ARROW_COLOR: u8 = 11;
const HOP_TICKS: u64 = 15;
const WIGGLE_TICKS: u64 = 5;
const BITE_TICKS: u64 = 40;
const FANG_CRIT_TICK: u64 = 8;
const KEYFRAME_TICKS: u64 = 80;
const KINETIC_COOLDOWN: u64 = 10;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Fx {
    /// `count` particles at `getRandomX(1)`, `getRandomY() + lift`, gaussian velocity of `spread`.
    Around {
        particle: &'static str,
        count: u8,
        lift: f64,
        spread: f64,
    },
    Poof,
    Portal,
    Drown,
    Honey(u8),
    ItemBreak(usize),
    WitchMagic,
    SnowballBreak,
    EggBreak,
    ArrowEffect,
    Fireworks,
    FoxEat,
    RabbitSprint,
    RoarPoof,
    KineticHit,
    Sound {
        name: &'static str,
        volume: f32,
    },
    FangsSound,
    CureSound,
    Animate(Anim),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anim {
    Attack(u8),
    Offer,
    StopOffer,
    Shake,
    StopShake,
    EatGrass,
    Hop,
    Ram(bool),
    Stun,
    Tendrils,
    KeyframeAttack,
    SonicBoom,
    Invulnerable,
    Peek,
    Wiggle,
    TentacleSync,
    Bite,
}

const fn around(particle: &'static str, count: u8, lift: f64, spread: f64) -> Fx {
    Fx::Around {
        particle,
        count,
        lift,
        spread,
    }
}

const HEARTS: Fx = around("minecraft:heart", 7, 0.5, 0.02);
const SMOKE: Fx = around("minecraft:smoke", 7, 0.5, 0.02);
const ALLAY_HEARTS: Fx = around("minecraft:heart", 3, 0.5, 0.02);
const DOLPHIN_HAPPY: Fx = around("minecraft:happy_villager", 7, 0.2, 0.01);
const VILLAGER_HEARTS: Fx = around("minecraft:heart", 5, 1.0, 0.02);
const VILLAGER_ANGRY: Fx = around("minecraft:angry_villager", 5, 1.0, 0.02);
const VILLAGER_HAPPY: Fx = around("minecraft:happy_villager", 5, 1.0, 0.02);
const VILLAGER_SWEAT: Fx = around("minecraft:splash", 5, 1.0, 0.02);

/// What vanilla's most-derived `handleEntityEvent` does for `event` on entity type `name`.
pub fn effects(name: &str, event: i8) -> &'static [Fx] {
    use Anim::*;
    use Fx::*;
    let n = name.strip_prefix("minecraft:").unwrap_or(name);
    let c = class_of(n);
    match (n, event) {
        ("allay", 18) => &[ALLAY_HEARTS],
        ("armadillo", 64) => &[
            Sound {
                name: "entity.armadillo.peek",
                volume: 1.0,
            },
            Animate(Peek),
        ],
        ("dolphin", 38) => &[DOLPHIN_HAPPY],
        ("ocelot", 41) => &[HEARTS],
        ("ocelot", 40) => &[SMOKE],
        ("fox", 45) => &[FoxEat],
        ("goat", 58) => &[Animate(Ram(true))],
        ("goat", 59) => &[Animate(Ram(false))],
        ("iron_golem", 4) => &[Animate(Attack(10))],
        ("iron_golem", 11) => &[Animate(Offer)],
        ("iron_golem", 34) => &[Animate(StopOffer)],
        ("rabbit", 1) => &[RabbitSprint, Animate(Hop)],
        ("sheep", 10) => &[Animate(EatGrass)],
        ("squid" | "glow_squid", 19) => &[Animate(TentacleSync)],
        ("wolf", 8) => &[Animate(Shake)],
        ("wolf", 56) => &[Animate(StopShake)],
        ("armor_stand", 32) => &[
            Sound {
                name: "entity.armor_stand.hit",
                volume: 0.3,
            },
            Animate(Wiggle),
        ],
        ("creaking", 66) => &[Animate(Invulnerable)],
        ("creaking", 4) => &[Animate(Attack(15)), Animate(KeyframeAttack)],
        ("hoglin" | "zoglin" | "ravager", 4) => &[Animate(Attack(10))],
        ("ravager", 39) => &[Animate(Stun)],
        ("ravager", 69) => &[RoarPoof],
        ("warden", 4) => &[Animate(KeyframeAttack)],
        ("warden", 61) => &[Animate(Tendrils)],
        ("warden", 62) => &[Animate(SonicBoom)],
        ("witch", 15) => &[WitchMagic],
        ("zombie_villager", 16) => &[CureSound],
        ("villager", 12) => &[VILLAGER_HEARTS],
        ("villager", 13) => &[VILLAGER_ANGRY],
        ("villager", 14) => &[VILLAGER_HAPPY],
        ("villager", 42) => &[VILLAGER_SWEAT],
        ("arrow", 0) => &[ArrowEffect],
        ("evoker_fangs", 4) => &[FangsSound, Animate(Bite)],
        ("firework_rocket", 17) => &[Fireworks],
        ("snowball", 3) => &[SnowballBreak],
        ("egg", 3) => &[EggBreak],
        ("snowball" | "egg", _) => &[],
        _ => {
            if event == 7 && (c.tamable || c.horse) {
                &[HEARTS]
            } else if event == 6 && (c.tamable || c.horse) {
                &[SMOKE]
            } else if event == 18 && c.animal {
                &[HEARTS]
            } else if event == 20 && c.mob {
                &[Poof]
            } else if c.living {
                living(event)
            } else if event == 53 {
                &[Honey(5)]
            } else {
                &[]
            }
        }
    }
}

fn living(event: i8) -> &'static [Fx] {
    use Fx::*;
    match event {
        2 => &[KineticHit],
        46 => &[Portal],
        53 => &[Honey(5)],
        54 => &[Honey(10)],
        60 => &[Poof],
        67 => &[Drown],
        47 => &[ItemBreak(0)],
        48 => &[ItemBreak(5)],
        49 => &[ItemBreak(4)],
        50 => &[ItemBreak(3)],
        51 => &[ItemBreak(2)],
        52 => &[ItemBreak(1)],
        65 => &[ItemBreak(6)],
        68 => &[ItemBreak(7)],
        _ => &[],
    }
}

/// The parts of an entity the effects read, for remote entities and the local player alike.
struct Body {
    name: &'static str,
    pos: [f64; 3],
    width: f64,
    height: f64,
    eye: f64,
    yaw: f32,
    pitch: f32,
    vel: [f64; 3],
    silent: bool,
    living_flags: u8,
    equipment: [ItemStack; 8],
    item: Option<ItemStack>,
    arrow_color: i32,
    source: SoundSource,
}

fn local_equipment(state: &ClientState) -> [ItemStack; 8] {
    let inv = &state.inventory;
    let menu = |index: i32| {
        mcr_client::inventory_index_to_menu_slot(index)
            .and_then(|i| inv.player.slots.get(i))
            .cloned()
            .unwrap_or_default()
    };
    let mut e: [ItemStack; 8] = Default::default();
    e[0] = inv.hotbar(state.player.held_slot).clone();
    e[1] = menu(36);
    e[2] = menu(37);
    e[3] = menu(38);
    e[4] = menu(39);
    e[5] = menu(40);
    e
}

impl Body {
    fn of(state: &ClientState, id: i32) -> Option<Self> {
        let data = mcr_data::v26_3();
        let source = crate::overlays::entity_sound_source(state, id);
        if id == state.player.entity_id {
            let p = &state.player;
            let pos = p.pos();
            let flags = match p.data.get(&LIVING_FLAGS) {
                Some(EntityDataValue::Byte(b)) => *b as u8,
                _ => 0,
            };
            return Some(Body {
                name: "minecraft:player",
                pos,
                width: 0.6,
                height: 1.8,
                eye: f64::from(p.physics.pose.eye_height()),
                yaw: p.physics.yaw,
                pitch: p.physics.pitch,
                vel: p.physics.vel,
                silent: false,
                living_flags: flags,
                equipment: local_equipment(state),
                item: None,
                arrow_color: -1,
                source,
            });
        }
        let e = state.entities.get(id)?;
        let name = data.entity_type_name(u32::try_from(e.kind).ok()?)?;
        let info = data.entity_type_info(e.kind as u32);
        let baby = matches!(e.data.get(&BABY), Some(EntityDataValue::Boolean(true)))
            && class_of(name.strip_prefix("minecraft:").unwrap_or(name)).mob;
        let scale = if baby { 0.5 } else { 1.0 };
        let (w, h, eye) = info.map_or((0.6, 1.8, 1.62), |i| {
            (
                f64::from(i.width),
                f64::from(i.height),
                f64::from(i.eye_height),
            )
        });
        Some(Body {
            name,
            pos: e.pos,
            width: w * scale,
            height: h * scale,
            eye: eye * scale,
            yaw: e.yaw,
            pitch: e.pitch,
            vel: e.velocity,
            silent: matches!(e.data.get(&SILENT), Some(EntityDataValue::Boolean(true))),
            living_flags: match e.data.get(&LIVING_FLAGS) {
                Some(EntityDataValue::Byte(b)) => *b as u8,
                _ => 0,
            },
            equipment: e.equipment.clone(),
            item: match e.data.get(&ITEM_DATA) {
                Some(EntityDataValue::ItemStack(s)) => Some(s.clone()),
                _ => None,
            },
            arrow_color: match e.data.get(&ARROW_COLOR) {
                Some(EntityDataValue::Int(c)) => *c,
                _ => -1,
            },
            source,
        })
    }
}

/// Small deterministic generator standing in for the entity's `RandomSource`.
#[derive(Debug, Clone)]
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn below(&mut self, n: u32) -> u32 {
        ((self.unit() * f64::from(n)) as u32).min(n.saturating_sub(1))
    }

    fn gaussian(&mut self) -> f64 {
        let (u, v) = (self.unit().max(1.0e-12), self.unit());
        (-2.0 * u.ln()).sqrt() * (std::f64::consts::TAU * v).cos()
    }

    fn triangle(&mut self, mode: f64, deviation: f64) -> f64 {
        mode + deviation * (self.unit() - self.unit())
    }
}

/// Per-entity animation clocks the events start, advanced like the entity's own `aiStep`.
#[derive(Debug, Clone, Copy, Default)]
struct Timers {
    attack: u8,
    offer: u16,
    shake: Option<(f32, f32)>,
    eat: u8,
    ram: bool,
    ram_ticks: u8,
    stun: u8,
    roar: u8,
    tendril: u8,
    tendril_prev: u8,
    keyframe_attack: Option<u64>,
    sonic_boom: Option<u64>,
    invulnerable: Option<u64>,
    peek: Option<u64>,
    hop: Option<u64>,
    wiggle: Option<u64>,
    bite: Option<u64>,
    tentacle: Option<u64>,
    kinetic: Option<u64>,
}

/// What the renderer needs from the event clocks for one entity this frame.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct EventView {
    pub anim: EventAnim,
    pub attack_ticks: f32,
    pub offer_ticks: f32,
    pub shake: f32,
}

pub struct EventCx<'a> {
    pub state: &'a ClientState,
    pub effects: &'a mut WorldEffects,
    pub audio: Option<&'a mut Audio>,
}

impl EventCx<'_> {
    fn play(&mut self, name: &str, pos: [f64; 3], volume: f32, pitch: f32, source: SoundSource) {
        if let Some(audio) = self.audio.as_deref_mut() {
            audio.play(name, Some(pos), volume, pitch, source);
        }
    }
}

#[derive(Debug)]
pub struct EntityEvents {
    timers: FxHashMap<i32, Timers>,
    tick: u64,
    rng: Rng,
}

impl Default for EntityEvents {
    fn default() -> Self {
        Self {
            timers: FxHashMap::default(),
            tick: 0,
            rng: Rng(0x9E37_79B9_7F4A_7C15),
        }
    }
}

fn age(now: u64, start: Option<u64>, limit: u64, partial: f32) -> Option<f32> {
    let start = start?;
    let a = now.checked_sub(start)?;
    (a <= limit).then_some(a as f32 + partial)
}

fn view_vector(pitch: f32, yaw: f32) -> [f64; 3] {
    let (p, y) = (f64::from(pitch).to_radians(), f64::from(yaw).to_radians());
    [-y.sin() * p.cos(), -p.sin(), y.cos() * p.cos()]
}

/// Vanilla `Vec3.xRot(-pitch).yRot(-yaw)`: a local offset turned to where the entity faces.
fn turn(v: [f64; 3], pitch: f32, yaw: f32) -> [f64; 3] {
    let a = -f64::from(pitch).to_radians();
    let (c, s) = (a.cos(), a.sin());
    let [x, y, z] = [v[0], v[1] * c + v[2] * s, v[2] * c - v[1] * s];
    let b = -f64::from(yaw).to_radians();
    let (c, s) = (b.cos(), b.sin());
    [x * c + z * s, y, z * c - x * s]
}

fn item_particle(stack: &ItemStack) -> Particle {
    Particle::Item(ItemStackTemplate {
        item: stack.item,
        count: 1,
        components: stack.components.clone(),
    })
}

fn sound_name(holder: &mcr_buf::IdOr<mcr_protocol::types::SoundEvent>) -> Option<String> {
    match holder {
        mcr_buf::IdOr::Id(id) => mcr_data::v26_3()
            .registry("minecraft:sound_event")
            .and_then(|r| r.name_of(u32::try_from(*id).ok()?))
            .map(|n| n.strip_prefix("minecraft:").unwrap_or(n).to_owned()),
        mcr_buf::IdOr::Inline(ev) => Some(ev.location.as_str().to_owned()),
    }
}

fn item_name(stack: &ItemStack) -> &'static str {
    u32::try_from(stack.item)
        .ok()
        .and_then(|i| mcr_data::v26_3().item_name(i))
        .unwrap_or("")
}

/// The item's `break_sound`: its own component, else the item's default.
fn break_sound(stack: &ItemStack) -> String {
    let patched = stack.components.added.iter().find_map(|c| match c {
        DataComponent::BreakSound(s) => sound_name(s),
        _ => None,
    });
    patched.unwrap_or_else(|| {
        match item_name(stack) {
            "minecraft:wolf_armor" => "item.wolf_armor.break",
            "minecraft:shield" => "item.shield.break",
            _ => "entity.item.break",
        }
        .to_owned()
    })
}

/// The `kinetic_weapon` hit sound of the item, falling back to the spears' defaults.
fn kinetic_hit_sound(stack: &ItemStack) -> Option<String> {
    let patched = stack.components.added.iter().find_map(|c| match c {
        DataComponent::KineticWeapon(k) => Some(k.hit_sound.as_ref().and_then(sound_name)),
        _ => None,
    });
    match patched {
        Some(sound) => sound,
        None => match item_name(stack) {
            "minecraft:wooden_spear" => Some("item.spear_wood.hit".to_owned()),
            n if n.ends_with("_spear") => Some("item.spear.hit".to_owned()),
            _ => None,
        },
    }
}

/// `PrimedTnt` and `MinecartTNT` puff smoke every tick while the fuse burns.
fn tnt_smoke(cx: &mut EventCx<'_>) {
    let p = particle_id("minecraft:smoke");
    for e in cx.state.entities.iter() {
        if e.fuse_remaining(1.0).is_some_and(|f| f > 0.0) {
            cx.effects
                .add_particle(&p, [e.pos[0], e.pos[1] + 0.5, e.pos[2]], [0.0; 3]);
        }
    }
}

impl EntityEvents {
    pub fn clear(&mut self) {
        self.timers.clear();
    }

    fn timers(&mut self, id: i32) -> Option<&mut Timers> {
        if self.timers.len() >= MAX_TIMERS && !self.timers.contains_key(&id) {
            return None;
        }
        Some(self.timers.entry(id).or_default())
    }

    pub fn on_event(&mut self, cx: &mut EventCx<'_>, id: i32, event: i8) {
        let Some(body) = Body::of(cx.state, id) else {
            return;
        };
        for fx in effects(body.name, event) {
            self.apply(cx, id, &body, *fx);
        }
    }

    fn random_x(&mut self, b: &Body) -> f64 {
        b.pos[0] + b.width * (2.0 * self.rng.unit() - 1.0)
    }

    fn random_z(&mut self, b: &Body) -> f64 {
        b.pos[2] + b.width * (2.0 * self.rng.unit() - 1.0)
    }

    fn random_y(&mut self, b: &Body) -> f64 {
        b.pos[1] + b.height * self.rng.unit()
    }

    fn apply(&mut self, cx: &mut EventCx<'_>, id: i32, b: &Body, fx: Fx) {
        let now = self.tick;
        match fx {
            Fx::Around {
                particle,
                count,
                lift,
                spread,
            } => {
                let p = particle_id(particle);
                for _ in 0..count {
                    let v = [0; 3].map(|_| self.rng.gaussian() * spread);
                    let at = [self.random_x(b), self.random_y(b) + lift, self.random_z(b)];
                    cx.effects.add_particle(&p, at, v);
                }
            }
            Fx::Poof => {
                let p = particle_id("minecraft:poof");
                for _ in 0..20 {
                    let v = [0; 3].map(|_| self.rng.gaussian() * 0.02);
                    let at = [
                        self.random_x(b) - v[0] * 10.0,
                        self.random_y(b) - v[1] * 10.0,
                        self.random_z(b) - v[2] * 10.0,
                    ];
                    cx.effects.add_particle(&p, at, v);
                }
            }
            Fx::Portal => {
                let p = particle_id("minecraft:portal");
                for _ in 0..128 {
                    let v = [0; 3].map(|_| (self.rng.unit() - 0.5) * 0.2);
                    let at = [
                        b.pos[0] + (self.rng.unit() - 0.5) * b.width * 2.0,
                        b.pos[1] + self.rng.unit() * b.height,
                        b.pos[2] + (self.rng.unit() - 0.5) * b.width * 2.0,
                    ];
                    cx.effects.add_particle(&p, at, v);
                }
            }
            Fx::Drown => {
                let p = particle_id("minecraft:bubble");
                for _ in 0..8 {
                    let o = [0; 3].map(|_| self.rng.triangle(0.0, 1.0));
                    let at = [0, 1, 2].map(|k| b.pos[k] + o[k]);
                    cx.effects.add_particle(&p, at, b.vel);
                }
            }
            Fx::Honey(count) => {
                let data = mcr_data::v26_3();
                let Some(state) = data
                    .block_by_name("minecraft:honey_block")
                    .and_then(|blk| data.block_info(blk))
                    .map(|i| i.default_state as i32)
                else {
                    return;
                };
                let p = Particle::BlockState {
                    kind: BlockParticleKind::Block,
                    state,
                };
                for _ in 0..count {
                    cx.effects.add_particle(&p, b.pos, [0.0; 3]);
                }
            }
            Fx::ItemBreak(slot) => {
                let Some(stack) = b.equipment.get(slot).filter(|s| !s.is_empty()) else {
                    return;
                };
                if !b.silent {
                    let pitch = 0.8 + self.rng.unit() as f32 * 0.4;
                    cx.play(&break_sound(stack), b.pos, 0.8, pitch, b.source);
                }
                self.item_burst(cx, b, &item_particle(stack));
            }
            Fx::FoxEat => {
                let Some(stack) = b.equipment.first().filter(|s| !s.is_empty()) else {
                    return;
                };
                let p = item_particle(stack);
                let look = view_vector(b.pitch, b.yaw);
                for _ in 0..8 {
                    let d = turn(
                        [
                            (self.rng.unit() - 0.5) * 0.1,
                            self.rng.unit() * 0.1 + 0.1,
                            0.0,
                        ],
                        b.pitch,
                        b.yaw,
                    );
                    let at = [b.pos[0] + look[0] / 2.0, b.pos[1], b.pos[2] + look[2] / 2.0];
                    cx.effects.add_particle(&p, at, [d[0], d[1] + 0.05, d[2]]);
                }
            }
            Fx::WitchMagic => {
                let p = particle_id("minecraft:witch");
                for _ in 0..self.rng.below(35) + 10 {
                    let at = [
                        b.pos[0] + self.rng.gaussian() * 0.13,
                        b.pos[1] + b.height + 0.5 + self.rng.gaussian() * 0.13,
                        b.pos[2] + self.rng.gaussian() * 0.13,
                    ];
                    cx.effects.add_particle(&p, at, [0.0; 3]);
                }
            }
            Fx::SnowballBreak => {
                let p = match &b.item {
                    Some(s) if !s.is_empty() && item_name(s) != "minecraft:snowball" => {
                        item_particle(s)
                    }
                    _ => particle_id("minecraft:item_snowball"),
                };
                for _ in 0..8 {
                    cx.effects.add_particle(&p, b.pos, [0.0; 3]);
                }
            }
            Fx::EggBreak => {
                let egg = mcr_data::v26_3()
                    .item_by_name("minecraft:egg")
                    .map_or(0, |i| i as i32);
                let stack = b
                    .item
                    .clone()
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| ItemStack::new(egg, 1));
                let p = item_particle(&stack);
                for _ in 0..8 {
                    let v = [0; 3].map(|_| (self.rng.unit() - 0.5) * 0.08);
                    cx.effects.add_particle(&p, b.pos, v);
                }
            }
            Fx::ArrowEffect => {
                if b.arrow_color == -1 {
                    return;
                }
                let p = Particle::Color {
                    kind: ColorParticleKind::EntityEffect,
                    argb: b.arrow_color | 0xFF00_0000u32 as i32,
                };
                for _ in 0..20 {
                    let at = [
                        b.pos[0] + b.width * (2.0 * self.rng.unit() - 1.0) * 0.5,
                        self.random_y(b),
                        b.pos[2] + b.width * (2.0 * self.rng.unit() - 1.0) * 0.5,
                    ];
                    cx.effects.add_particle(&p, at, [0.0; 3]);
                }
            }
            Fx::Fireworks => {
                let explosions = b
                    .item
                    .iter()
                    .flat_map(|s| s.components.added.iter())
                    .find_map(|c| match c {
                        DataComponent::Fireworks(f) => Some(f.explosions.0.as_slice()),
                        _ => None,
                    })
                    .unwrap_or(&[]);
                cx.effects.fireworks(b.pos, b.vel, explosions, !b.silent);
            }
            Fx::RabbitSprint => self.sprint_particle(cx, b),
            Fx::RoarPoof => {
                let p = particle_id("minecraft:poof");
                let center = [b.pos[0], b.pos[1] + b.height / 2.0, b.pos[2]];
                for _ in 0..40 {
                    let v = [0; 3].map(|_| self.rng.gaussian() * 0.2);
                    cx.effects.add_particle(&p, center, v);
                }
            }
            Fx::KineticHit => {
                let Some(t) = self.timers(id) else { return };
                if t.kinetic
                    .is_some_and(|last| now.saturating_sub(last) <= KINETIC_COOLDOWN)
                {
                    return;
                }
                t.kinetic = Some(now);
                let hand = if b.living_flags & 2 != 0 { 5 } else { 0 };
                if let Some(sound) = b.equipment.get(hand).and_then(kinetic_hit_sound) {
                    cx.play(&sound, b.pos, 1.0, 1.0, b.source);
                }
            }
            Fx::Sound { name, volume } => cx.play(name, b.pos, volume, 1.0, b.source),
            Fx::FangsSound => {
                if !b.silent {
                    let pitch = self.rng.unit() as f32 * 0.2 + 0.85;
                    cx.play("entity.evoker_fangs.attack", b.pos, 1.0, pitch, b.source);
                }
            }
            Fx::CureSound => {
                if !b.silent {
                    let volume = 1.0 + self.rng.unit() as f32;
                    let pitch = self.rng.unit() as f32 * 0.7 + 0.3;
                    let at = [b.pos[0], b.pos[1] + b.eye, b.pos[2]];
                    cx.play("entity.zombie_villager.cure", at, volume, pitch, b.source);
                }
            }
            Fx::Animate(a) => self.start(id, a),
        }
    }

    /// Vanilla `LivingEntity.spawnItemParticles` with its five pieces in front of the eyes.
    fn item_burst(&mut self, cx: &mut EventCx<'_>, b: &Body, p: &Particle) {
        for _ in 0..5 {
            let d = turn(
                [
                    (self.rng.unit() - 0.5) * 0.1,
                    self.rng.unit() * 0.1 + 0.1,
                    0.0,
                ],
                b.pitch,
                b.yaw,
            );
            let o = turn(
                [
                    (self.rng.unit() - 0.5) * 0.3,
                    -self.rng.unit() * 0.6 - 0.3,
                    0.6,
                ],
                b.pitch,
                b.yaw,
            );
            let at = [b.pos[0] + o[0], b.pos[1] + b.eye + o[1], b.pos[2] + o[2]];
            cx.effects.add_particle(p, at, [d[0], d[1] + 0.05, d[2]]);
        }
    }

    /// Vanilla `Entity.spawnSprintParticle`: a crumb of the block underfoot kicked backwards.
    fn sprint_particle(&mut self, cx: &mut EventCx<'_>, b: &Body) {
        let data = mcr_data::v26_3();
        let [x, y, z] = [b.pos[0], b.pos[1] - 0.2, b.pos[2]].map(|v| v.floor() as i32);
        let Some(state) = cx.state.chunks.block(x, y, z) else {
            return;
        };
        let visible = data
            .block_state(u32::from(state.0))
            .is_some_and(|s| !s.is_air() && s.fluid.is_none());
        if !visible {
            return;
        }
        let px =
            (b.pos[0] + (self.rng.unit() - 0.5) * b.width).clamp(f64::from(x), f64::from(x) + 1.0);
        let pz =
            (b.pos[2] + (self.rng.unit() - 0.5) * b.width).clamp(f64::from(z), f64::from(z) + 1.0);
        let p = Particle::BlockState {
            kind: BlockParticleKind::Block,
            state: i32::from(state.0),
        };
        cx.effects.add_particle(
            &p,
            [px, b.pos[1] + 0.1, pz],
            [b.vel[0] * -4.0, 1.5, b.vel[2] * -4.0],
        );
    }

    fn start(&mut self, id: i32, a: Anim) {
        let now = self.tick;
        let Some(t) = self.timers(id) else { return };
        match a {
            Anim::Attack(ticks) => t.attack = ticks,
            Anim::Offer => t.offer = 400,
            Anim::StopOffer => t.offer = 0,
            Anim::Shake => t.shake = Some((0.0, 0.0)),
            Anim::StopShake => t.shake = None,
            Anim::EatGrass => t.eat = 40,
            Anim::Hop => t.hop = Some(now),
            Anim::Ram(on) => t.ram = on,
            Anim::Stun => t.stun = 40,
            Anim::Tendrils => t.tendril = 10,
            Anim::KeyframeAttack => t.keyframe_attack = Some(now),
            Anim::SonicBoom => t.sonic_boom = Some(now),
            Anim::Invulnerable => t.invulnerable = Some(now),
            Anim::Peek => t.peek = Some(now),
            Anim::Wiggle => t.wiggle = Some(now),
            Anim::TentacleSync => t.tentacle = Some(now),
            Anim::Bite => t.bite = Some(now),
        }
    }

    /// One 20 Hz step of every clock, with the particles vanilla's entity ticks spawn meanwhile.
    pub fn tick(&mut self, cx: &mut EventCx<'_>) {
        self.tick += 1;
        let now = self.tick;
        let local = cx.state.player.entity_id;
        self.timers
            .retain(|id, _| *id == local || cx.state.entities.get(*id).is_some());
        let mut timers = std::mem::take(&mut self.timers);
        for (id, t) in &mut timers {
            t.attack = t.attack.saturating_sub(1);
            t.offer = t.offer.saturating_sub(1);
            t.eat = t.eat.saturating_sub(1);
            t.ram_ticks = if t.ram {
                (t.ram_ticks + 1).min(20)
            } else {
                t.ram_ticks.saturating_sub(2)
            };
            t.roar = t.roar.saturating_sub(1);
            if t.stun > 0 {
                t.stun -= 1;
                if self.rng.below(6) == 0
                    && let Some(b) = Body::of(cx.state, *id)
                {
                    self.stun_particle(cx, &b);
                }
                if t.stun == 0 {
                    t.roar = 20;
                }
            }
            t.tendril_prev = t.tendril;
            t.tendril = t.tendril.saturating_sub(1);
            if let Some((_, cur)) = t.shake {
                if cur >= 2.0 {
                    t.shake = None;
                } else {
                    t.shake = Some((cur, cur + 0.05));
                    if cur + 0.05 > 0.4
                        && let Some(b) = Body::of(cx.state, *id)
                    {
                        self.shake_splash(cx, &b, cur + 0.05);
                    }
                }
            }
            if t.bite.is_some_and(|s| now - s == FANG_CRIT_TICK)
                && let Some(b) = Body::of(cx.state, *id)
            {
                self.fang_crits(cx, &b);
            }
        }
        self.timers = timers;
        tnt_smoke(cx);
    }

    fn stun_particle(&mut self, cx: &mut EventCx<'_>, b: &Body) {
        let yaw = f64::from(b.yaw).to_radians();
        let at = [
            b.pos[0] - b.width * yaw.sin() + (self.rng.unit() * 0.6 - 0.3),
            b.pos[1] + b.height - 0.3,
            b.pos[2] + b.width * yaw.cos() + (self.rng.unit() * 0.6 - 0.3),
        ];
        let p = Particle::Color {
            kind: ColorParticleKind::EntityEffect,
            argb: 0xFF7F_8392u32 as i32,
        };
        cx.effects.add_particle(&p, at, [0.0; 3]);
    }

    fn shake_splash(&mut self, cx: &mut EventCx<'_>, b: &Body, shake: f32) {
        let count = (((shake - 0.4) * std::f32::consts::PI).sin() * 7.0) as i32;
        let p = particle_id("minecraft:splash");
        for _ in 0..count {
            let xo = (self.rng.unit() * 2.0 - 1.0) * b.width * 0.5;
            let zo = (self.rng.unit() * 2.0 - 1.0) * b.width * 0.5;
            let at = [b.pos[0] + xo, b.pos[1] + 0.8, b.pos[2] + zo];
            cx.effects.add_particle(&p, at, b.vel);
        }
    }

    fn fang_crits(&mut self, cx: &mut EventCx<'_>, b: &Body) {
        let p = particle_id("minecraft:crit");
        for _ in 0..12 {
            let at = [
                b.pos[0] + (self.rng.unit() * 2.0 - 1.0) * b.width * 0.5,
                b.pos[1] + 0.05 + self.rng.unit() + 1.0,
                b.pos[2] + (self.rng.unit() * 2.0 - 1.0) * b.width * 0.5,
            ];
            let v = [
                (self.rng.unit() * 2.0 - 1.0) * 0.3,
                0.3 + self.rng.unit() * 0.3,
                (self.rng.unit() * 2.0 - 1.0) * 0.3,
            ];
            cx.effects.add_particle(&p, at, v);
        }
    }

    pub fn view(&self, id: i32, partial: f32) -> EventView {
        let Some(t) = self.timers.get(&id) else {
            return EventView::default();
        };
        let now = self.tick;
        let left = |ticks: u8| {
            if ticks > 0 {
                f32::from(ticks) - partial
            } else {
                0.0
            }
        };
        EventView {
            anim: EventAnim {
                eat: left(t.eat),
                ram: f32::from(t.ram_ticks),
                stunned: left(t.stun),
                roar: if t.roar > 0 {
                    (20.0 - f32::from(t.roar) + partial) / 20.0
                } else {
                    0.0
                },
                tendril: (f32::from(t.tendril_prev)
                    + (f32::from(t.tendril) - f32::from(t.tendril_prev)) * partial)
                    / 10.0,
                attack: age(now, t.keyframe_attack, KEYFRAME_TICKS, partial),
                sonic_boom: age(now, t.sonic_boom, KEYFRAME_TICKS, partial),
                invulnerable: age(now, t.invulnerable, KEYFRAME_TICKS, partial),
                peek: age(now, t.peek, KEYFRAME_TICKS, partial),
                hop: age(now, t.hop, HOP_TICKS, partial),
                wiggle: age(now, t.wiggle, WIGGLE_TICKS, partial),
                bite: age(now, t.bite, BITE_TICKS, partial).map_or(0.0, |a| (a / 20.0).min(1.0)),
                tentacle_sync: age(now, t.tentacle, u64::MAX, partial),
            },
            attack_ticks: left(t.attack),
            offer_ticks: f32::from(t.offer),
            shake: t
                .shake
                .map_or(0.0, |(prev, cur)| prev + (cur - prev) * partial),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Anim::*;
    use Fx::*;

    fn world() -> Option<(WorldEffects, ClientState)> {
        let jar = std::path::Path::new("../../.cache/versions/26.3/client.jar");
        if !jar.exists() {
            return None;
        }
        let assets = std::sync::Arc::new(crate::assets::Assets::load(jar).ok()?);
        let state = ClientState::new(&mcr_client::ClientConfig::default(), uuid::Uuid::nil());
        Some((WorldEffects::new(assets), state))
    }

    fn spawn(state: &mut ClientState, id: i32, name: &str) {
        let kind = mcr_data::v26_3().entity_type_by_name(name).unwrap() as i32;
        state.entities.insert(mcr_client::Entity::new(
            id,
            uuid::Uuid::nil(),
            kind,
            [0.5, 64.0, 0.5],
        ));
    }

    #[test]
    fn failed_and_successful_taming_spawn_real_particles() {
        let Some((mut fx, mut state)) = world() else {
            return;
        };
        spawn(&mut state, 5, "minecraft:wolf");
        let mut ev = EntityEvents::default();
        for (event, expected) in [(6, 7), (7, 14)] {
            let mut cx = EventCx {
                state: &state,
                effects: &mut fx,
                audio: None,
            };
            ev.on_event(&mut cx, 5, event);
            assert_eq!(fx.particles.count(), expected);
        }
    }

    #[test]
    fn shaking_wolf_animates_and_drips_for_two_seconds() {
        let Some((mut fx, mut state)) = world() else {
            return;
        };
        spawn(&mut state, 5, "minecraft:wolf");
        let mut ev = EntityEvents::default();
        let mut cx = EventCx {
            state: &state,
            effects: &mut fx,
            audio: None,
        };
        ev.on_event(&mut cx, 5, 8);
        for _ in 0..10 {
            ev.tick(&mut cx);
        }
        assert!((ev.view(5, 1.0).shake - 0.5).abs() < 1e-4);
        assert!((ev.view(5, 0.0).shake - 0.45).abs() < 1e-4);
        for _ in 0..40 {
            ev.tick(&mut cx);
        }
        assert_eq!(ev.view(5, 0.0).shake, 0.0);
        assert!(fx.particles.count() > 0);
    }

    #[test]
    fn firework_rocket_with_explosions_bursts_after_a_tick() {
        use mcr_protocol::types::component::{FireworkExplosion, FireworkShape, Fireworks};
        let Some((mut fx, mut state)) = world() else {
            return;
        };
        spawn(&mut state, 9, "minecraft:firework_rocket");
        let rocket = mcr_data::v26_3()
            .item_by_name("minecraft:firework_rocket")
            .unwrap();
        let mut stack = ItemStack::new(rocket as i32, 1);
        stack
            .components
            .added
            .push(DataComponent::Fireworks(Fireworks {
                flight_duration: 1,
                explosions: mcr_buf::LengthPrefixed(vec![FireworkExplosion {
                    shape: FireworkShape::SmallBall,
                    colors: vec![0xFF0000],
                    fade_colors: vec![],
                    has_trail: false,
                    has_twinkle: false,
                }]),
            }));
        state
            .entities
            .get_mut(9)
            .unwrap()
            .data
            .insert(ITEM_DATA, EntityDataValue::ItemStack(stack));
        let mut ev = EntityEvents::default();
        let mut cx = EventCx {
            state: &state,
            effects: &mut fx,
            audio: None,
        };
        ev.on_event(&mut cx, 9, 17);
        fx.tick(
            &state,
            mcr_render::glam::DVec3::new(0.0, 64.0, 0.0),
            &mcr_settings::Options::default(),
        );
        assert!(fx.particles.count() > 50, "{}", fx.particles.count());
        assert_eq!(fx.sounds.len(), 1);
        assert_eq!(fx.sounds[0].name, "entity.firework_rocket.blast");
    }

    #[test]
    fn taming_events_show_smoke_then_hearts() {
        for pet in [
            "minecraft:wolf",
            "minecraft:cat",
            "minecraft:parrot",
            "minecraft:horse",
        ] {
            assert_eq!(effects(pet, 6), &[SMOKE], "{pet}");
            assert_eq!(effects(pet, 7), &[HEARTS], "{pet}");
        }
        assert_eq!(effects("minecraft:ocelot", 40), &[SMOKE]);
        assert_eq!(effects("minecraft:ocelot", 41), &[HEARTS]);
        assert!(effects("minecraft:zombie", 7).is_empty());
    }

    #[test]
    fn love_hearts_follow_the_class_chain() {
        assert_eq!(effects("minecraft:cow", 18), &[HEARTS]);
        assert_eq!(effects("minecraft:wolf", 18), &[HEARTS]);
        assert_eq!(effects("minecraft:allay", 18), &[ALLAY_HEARTS]);
        assert!(effects("minecraft:zombie", 18).is_empty());
    }

    #[test]
    fn class_specific_events_do_not_leak_to_other_types() {
        assert_eq!(effects("minecraft:iron_golem", 4), &[Animate(Attack(10))]);
        assert_eq!(effects("minecraft:warden", 4), &[Animate(KeyframeAttack)]);
        assert!(effects("minecraft:zombie", 4).is_empty());
        assert_eq!(effects("minecraft:snowball", 3), &[SnowballBreak]);
        assert!(effects("minecraft:zombie", 3).is_empty());
        assert!(effects("minecraft:snowball", 53).is_empty());
        assert_eq!(effects("minecraft:arrow", 53), &[Honey(5)]);
        assert_eq!(effects("minecraft:zombie", 20), &[Poof]);
        assert!(effects("minecraft:armor_stand", 20).is_empty());
        assert_eq!(effects("minecraft:player", 47), &[ItemBreak(0)]);
        assert_eq!(effects("minecraft:wolf", 65), &[ItemBreak(6)]);
    }

    #[test]
    fn break_events_map_to_their_equipment_slots() {
        let slots = [
            (47, 0),
            (48, 5),
            (49, 4),
            (50, 3),
            (51, 2),
            (52, 1),
            (65, 6),
            (68, 7),
        ];
        for (event, slot) in slots {
            assert_eq!(effects("minecraft:zombie", event), &[ItemBreak(slot)]);
        }
    }

    #[test]
    fn villager_moods_use_their_particles() {
        let p = |e| match effects("minecraft:villager", e) {
            [Around { particle, .. }] => *particle,
            other => panic!("{other:?}"),
        };
        assert_eq!(p(12), "minecraft:heart");
        assert_eq!(p(13), "minecraft:angry_villager");
        assert_eq!(p(14), "minecraft:happy_villager");
        assert_eq!(p(42), "minecraft:splash");
        assert!(effects("minecraft:wandering_trader", 13).is_empty());
    }

    #[test]
    fn clocks_count_down_like_ai_step() {
        let mut ev = EntityEvents::default();
        ev.start(1, Attack(10));
        ev.start(1, EatGrass);
        ev.start(1, Shake);
        ev.start(1, Stun);
        assert_eq!(ev.view(1, 0.0).attack_ticks, 10.0);
        for _ in 0..3 {
            ev.timers.get_mut(&1).unwrap().attack -= 1;
        }
        assert_eq!(ev.view(1, 0.5).attack_ticks, 6.5);
        assert_eq!(ev.view(1, 0.0).anim.eat, 40.0);
        assert!(ev.view(2, 0.0) == EventView::default());
    }

    #[test]
    fn stun_turns_into_a_roar_and_bites_progress() {
        let mut ev = EntityEvents::default();
        ev.start(1, Stun);
        ev.start(1, Bite);
        let t = ev.timers.get_mut(&1).unwrap();
        t.stun = 1;
        t.stun -= 1;
        t.roar = 20;
        ev.tick = 10;
        let v = ev.view(1, 0.0);
        assert_eq!(v.anim.stunned, 0.0);
        assert_eq!(v.anim.roar, 0.0);
        assert!((v.anim.bite - 0.5).abs() < 1e-6);
        ev.tick = 100;
        assert_eq!(ev.view(1, 0.0).anim.bite, 0.0);
    }

    #[test]
    fn local_offsets_turn_with_the_entity() {
        let v = turn([0.0, 0.0, 1.0], 0.0, 90.0);
        assert!((v[0] + 1.0).abs() < 1e-9 && v[2].abs() < 1e-9);
        let look = view_vector(0.0, 90.0);
        assert!((look[0] + 1.0).abs() < 1e-9);
        let down = view_vector(90.0, 0.0);
        assert!((down[1] + 1.0).abs() < 1e-9);
    }

    #[test]
    fn break_and_kinetic_sounds_fall_back_to_item_defaults() {
        let data = mcr_data::v26_3();
        let stack = |n: &str| ItemStack::new(data.item_by_name(n).unwrap() as i32, 1);
        assert_eq!(
            break_sound(&stack("minecraft:diamond_pickaxe")),
            "entity.item.break"
        );
        assert_eq!(break_sound(&stack("minecraft:shield")), "item.shield.break");
        assert_eq!(
            kinetic_hit_sound(&stack("minecraft:wooden_spear")).as_deref(),
            Some("item.spear_wood.hit")
        );
        assert_eq!(
            kinetic_hit_sound(&stack("minecraft:iron_spear")).as_deref(),
            Some("item.spear.hit")
        );
        assert_eq!(kinetic_hit_sound(&stack("minecraft:stick")), None);
    }
}
