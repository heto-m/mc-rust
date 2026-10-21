//! Vanilla `LevelEventHandler`: the sounds and particles of the `level_event` packet. Simple sounds
//! are one row each in [`EVENTS`]; the rows name the particle routine that runs with them.

use mcr_audio::SoundSource;
use mcr_buf::BlockPos;
use mcr_client::ClientState;
use mcr_protocol::types::Particle;
use mcr_protocol::types::item::ItemStackTemplate;
use mcr_protocol::types::particle::{BlockParticleKind, SpellParticleKind};

use crate::audio::{Audio, BlockSound};
use crate::effects::{WorldEffects, particle_id};

/// How a row's pitch is drawn from the level's random source.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Pitch {
    Fixed(f32),
    /// `(r - r) * 0.2 + 1`.
    Spread,
    /// `r * 0.1 + 0.9`.
    Narrow,
    /// `base + (r - r) * width`.
    Hiss(f32, f32),
    /// `base + r * width`.
    Range(f32, f32),
    /// `(1 + (r - r) * 0.2) * 0.7`.
    Gateway,
    /// `0.4 / (r * 0.4 + 0.8)`.
    Throw,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Snd {
    pub name: &'static str,
    pub source: SoundSource,
    pub volume: f32,
    pub pitch: Pitch,
}

/// The particle (or block-dependent sound) routine of a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fx {
    None,
    /// Handled by the jukebox code (1010, 1011).
    Jukebox,
    Extinguish,
    PortalTravel,
    Composter,
    LavaFizz,
    TorchBurnout,
    PortalFrameFill,
    Drip,
    Growth,
    Shoot(&'static str),
    DestroyBlock,
    PotionSplash {
        instant: bool,
    },
    EyeOfEnder,
    MobSpawn,
    DragonBreath,
    Explosion,
    Evaporate,
    InBlock(&'static str),
    Smash,
    BlockParticles,
    Teleport([i32; 3]),
    EndermanTeleport,
    Breaking {
        sound: bool,
    },
    GatewaySpawn,
    Spark,
    Faces(&'static str, i32, i32),
    SculkCharge,
    Shriek,
    Brush,
    TrialSpawn,
    Detect(&'static str),
    Eject,
    VaultActivate,
    VaultDeactivate,
    Cobweb,
    BecomeOminous,
}

impl Fx {
    /// The main particle the routine spawns, for the table test.
    #[cfg(test)]
    pub fn particle(self) -> Option<&'static str> {
        Some(match self {
            Fx::None | Fx::Jukebox | Fx::Extinguish | Fx::PortalTravel => return None,
            Fx::Composter => "composter",
            Fx::LavaFizz => "large_smoke",
            Fx::TorchBurnout | Fx::PortalFrameFill => "smoke",
            Fx::Drip => "dripping_dripstone_water",
            Fx::Growth | Fx::InBlock(_) => "happy_villager",
            Fx::Shoot(p) | Fx::Faces(p, ..) | Fx::Detect(p) => p,
            Fx::DestroyBlock | Fx::BlockParticles | Fx::Breaking { .. } | Fx::Brush => "block",
            Fx::PotionSplash { instant: false } => "effect",
            Fx::PotionSplash { instant: true } => "instant_effect",
            Fx::EyeOfEnder | Fx::Teleport(_) | Fx::EndermanTeleport => "portal",
            Fx::MobSpawn | Fx::TrialSpawn => "flame",
            Fx::DragonBreath => "dragon_breath",
            Fx::Explosion => "explosion",
            Fx::Evaporate => "cloud",
            Fx::Smash => "dust_pillar",
            Fx::GatewaySpawn => "explosion_emitter",
            Fx::Spark => "electric_spark",
            Fx::SculkCharge => "sculk_charge",
            Fx::Shriek => "shriek",
            Fx::Eject => "small_flame",
            Fx::VaultActivate | Fx::VaultDeactivate => "small_flame",
            Fx::Cobweb => "poof",
            Fx::BecomeOminous => "trial_omen",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Row {
    pub id: i32,
    pub sound: Option<Snd>,
    pub fx: Fx,
}

const fn row(id: i32, sound: Option<Snd>, fx: Fx) -> Row {
    Row { id, sound, fx }
}

const fn snd(name: &'static str, source: SoundSource, volume: f32, pitch: Pitch) -> Option<Snd> {
    Some(Snd {
        name,
        source,
        volume,
        pitch,
    })
}

use Pitch::{Fixed, Narrow, Spread};
use SoundSource::{Blocks as B, Hostile as H, Neutral as N};

#[rustfmt::skip]
pub const EVENTS: &[Row] = &[
    row(1000, snd("block.dispenser.dispense", B, 1.0, Fixed(1.0)), Fx::None),
    row(1001, snd("block.dispenser.fail", B, 1.0, Fixed(1.2)), Fx::None),
    row(1002, snd("block.dispenser.launch", B, 1.0, Fixed(1.2)), Fx::None),
    row(1004, snd("entity.firework_rocket.shoot", N, 1.0, Fixed(1.2)), Fx::None),
    row(1009, None, Fx::Extinguish),
    row(1010, None, Fx::Jukebox),
    row(1011, None, Fx::Jukebox),
    row(1015, snd("entity.ghast.warn", H, 10.0, Spread), Fx::None),
    row(1016, snd("entity.ghast.shoot", H, 10.0, Spread), Fx::None),
    row(1017, snd("entity.ender_dragon.shoot", H, 10.0, Spread), Fx::None),
    row(1018, snd("entity.blaze.shoot", H, 2.0, Spread), Fx::None),
    row(1019, snd("entity.zombie.attack_wooden_door", H, 2.0, Spread), Fx::None),
    row(1020, snd("entity.zombie.attack_iron_door", H, 2.0, Spread), Fx::None),
    row(1021, snd("entity.zombie.break_wooden_door", H, 2.0, Spread), Fx::None),
    row(1022, snd("entity.wither.break_block", H, 2.0, Spread), Fx::None),
    row(1024, snd("entity.wither.shoot", H, 2.0, Spread), Fx::None),
    row(1025, snd("entity.bat.takeoff", N, 0.05, Spread), Fx::None),
    row(1026, snd("entity.zombie.infect", H, 2.0, Spread), Fx::None),
    row(1027, snd("entity.zombie_villager.converted", H, 2.0, Spread), Fx::None),
    row(1029, snd("block.anvil.destroy", B, 1.0, Narrow), Fx::None),
    row(1030, snd("block.anvil.use", B, 1.0, Narrow), Fx::None),
    row(1031, snd("block.anvil.land", B, 0.3, Narrow), Fx::None),
    row(1032, None, Fx::PortalTravel),
    row(1033, snd("block.chorus_flower.grow", B, 1.0, Fixed(1.0)), Fx::None),
    row(1034, snd("block.chorus_flower.death", B, 1.0, Fixed(1.0)), Fx::None),
    row(1035, snd("block.brewing_stand.brew", B, 1.0, Fixed(1.0)), Fx::None),
    row(1039, snd("entity.phantom.bite", H, 0.3, Narrow), Fx::None),
    row(1040, snd("entity.zombie.converted_to_drowned", H, 2.0, Spread), Fx::None),
    row(1041, snd("entity.husk.converted_to_zombie", H, 2.0, Spread), Fx::None),
    row(1042, snd("block.grindstone.use", B, 1.0, Narrow), Fx::None),
    row(1043, snd("item.book.page_turn", B, 1.0, Narrow), Fx::None),
    row(1044, snd("block.smithing_table.use", B, 1.0, Narrow), Fx::None),
    row(1045, snd("block.pointed_dripstone.land", B, 2.0, Narrow), Fx::None),
    row(1046, snd("block.pointed_dripstone.drip_lava_into_cauldron", B, 2.0, Narrow), Fx::None),
    row(1047, snd("block.pointed_dripstone.drip_water_into_cauldron", B, 2.0, Narrow), Fx::None),
    row(1048, snd("entity.skeleton.converted_to_stray", H, 2.0, Spread), Fx::None),
    row(1049, snd("block.crafter.craft", B, 1.0, Fixed(1.0)), Fx::None),
    row(1050, snd("block.crafter.fail", B, 1.0, Fixed(1.0)), Fx::None),
    row(1051, snd("entity.wind_charge.throw", B, 0.5, Pitch::Throw), Fx::None),
    row(1052, snd("block.sulfur_spike.land", B, 2.0, Narrow), Fx::None),
    row(1053, snd("entity.splash_potion.break", N, 1.0, Narrow), Fx::None),
    row(1054, snd("entity.splash_potion.break", N, 1.0, Narrow), Fx::None),
    row(1500, None, Fx::Composter),
    row(1501, snd("block.lava.extinguish", B, 0.5, Pitch::Hiss(2.6, 0.8)), Fx::LavaFizz),
    row(1502, snd("block.redstone_torch.burnout", B, 0.5, Pitch::Hiss(2.6, 0.8)), Fx::TorchBurnout),
    row(1503, snd("block.end_portal_frame.fill", B, 1.0, Fixed(1.0)), Fx::PortalFrameFill),
    row(1504, None, Fx::Drip),
    row(1505, snd("item.bone_meal.use", B, 1.0, Fixed(1.0)), Fx::Growth),
    row(2000, None, Fx::Shoot("smoke")),
    row(2001, None, Fx::DestroyBlock),
    row(2002, None, Fx::PotionSplash { instant: false }),
    row(2003, None, Fx::EyeOfEnder),
    row(2004, None, Fx::MobSpawn),
    row(2006, None, Fx::DragonBreath),
    row(2007, None, Fx::PotionSplash { instant: true }),
    row(2008, None, Fx::Explosion),
    row(2009, None, Fx::Evaporate),
    row(2010, None, Fx::Shoot("white_smoke")),
    row(2011, None, Fx::InBlock("happy_villager")),
    row(2012, None, Fx::InBlock("happy_villager")),
    row(2013, None, Fx::Smash),
    row(2014, None, Fx::BlockParticles),
    row(2015, None, Fx::Teleport([16, 8, 16])),
    row(2016, None, Fx::Teleport([8, 8, 8])),
    row(2017, None, Fx::Teleport([127, 127, 127])),
    row(2018, None, Fx::EndermanTeleport),
    row(2019, None, Fx::Breaking { sound: false }),
    row(2020, None, Fx::Breaking { sound: true }),
    row(3000, snd("block.end_gateway.spawn", B, 10.0, Pitch::Gateway), Fx::GatewaySpawn),
    row(3001, snd("entity.ender_dragon.growl", H, 64.0, Pitch::Range(0.8, 0.3)), Fx::None),
    row(3002, None, Fx::Spark),
    row(3003, None, Fx::Faces("wax_on", 3, 5)),
    row(3004, None, Fx::Faces("wax_off", 3, 5)),
    row(3005, None, Fx::Faces("scrape", 3, 5)),
    row(3006, None, Fx::SculkCharge),
    row(3007, None, Fx::Shriek),
    row(3008, None, Fx::Brush),
    row(3009, None, Fx::Faces("egg_crack", 3, 6)),
    row(3011, None, Fx::TrialSpawn),
    row(3012, snd("block.trial_spawner.spawn_mob", B, 1.0, Spread), Fx::TrialSpawn),
    row(3013, snd("block.trial_spawner.detect_player", B, 1.0, Spread), Fx::Detect("trial_spawner_detection")),
    row(3014, snd("block.trial_spawner.eject_item", B, 1.0, Spread), Fx::Eject),
    row(3015, None, Fx::VaultActivate),
    row(3016, snd("block.vault.deactivate", B, 1.0, Spread), Fx::VaultDeactivate),
    row(3017, None, Fx::Eject),
    row(3018, snd("block.cobweb.place", B, 1.0, Spread), Fx::Cobweb),
    row(3019, snd("block.trial_spawner.detect_player", B, 1.0, Spread), Fx::Detect("trial_spawner_detection_ominous")),
    row(3020, None, Fx::BecomeOminous),
    row(3021, snd("block.trial_spawner.spawn_item", B, 1.0, Spread), Fx::TrialSpawn),
];

/// `globalLevelEvent`: heard from anywhere, two blocks from the camera towards the event.
pub const GLOBAL_EVENTS: &[(i32, &str, f32)] = &[
    (1023, "entity.wither.spawn", 1.0),
    (1028, "entity.ender_dragon.death", 5.0),
    (1038, "block.end_portal.spawn", 1.0),
];

pub fn lookup(id: i32) -> Option<&'static Row> {
    EVENTS
        .binary_search_by_key(&id, |r| r.id)
        .ok()
        .map(|i| &EVENTS[i])
}

/// Stands in for the level's `RandomSource`.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    pub fn f64(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn f32(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }

    fn gaussian(&mut self) -> f64 {
        let (u, v) = (self.f64().max(1.0e-12), self.f64());
        (-2.0 * u.ln()).sqrt() * (std::f64::consts::TAU * v).cos()
    }

    fn between(&mut self, lo: f64, hi: f64) -> f64 {
        lo + self.f64() * (hi - lo)
    }

    /// `UniformInt.of(lo, hi)`, inclusive.
    fn uniform(&mut self, lo: i32, hi: i32) -> i32 {
        lo + (self.f64() * f64::from(hi - lo + 1)) as i32
    }

    pub fn pitch(&mut self, p: Pitch) -> f32 {
        match p {
            Pitch::Fixed(v) => v,
            Pitch::Spread => (self.f32() - self.f32()) * 0.2 + 1.0,
            Pitch::Narrow => self.f32() * 0.1 + 0.9,
            Pitch::Hiss(base, width) => base + (self.f32() - self.f32()) * width,
            Pitch::Range(base, width) => base + self.f32() * width,
            Pitch::Gateway => (1.0 + (self.f32() - self.f32()) * 0.2) * 0.7,
            Pitch::Throw => 0.4 / (self.f32() * 0.4 + 0.8),
        }
    }
}

const STEPS: [[i32; 3]; 6] = [
    [0, -1, 0],
    [0, 1, 0],
    [0, 0, -1],
    [0, 0, 1],
    [-1, 0, 0],
    [1, 0, 0],
];
const SHRIEKER_TOP: f64 = 0.5;
const ENDERMAN_SIZE: (f64, f64) = (0.6, 2.9);

fn corner(pos: BlockPos) -> [f64; 3] {
    [f64::from(pos.x), f64::from(pos.y), f64::from(pos.z)]
}

fn center(pos: BlockPos) -> [f64; 3] {
    corner(pos).map(|v| v + 0.5)
}

fn simple(name: &str) -> Particle {
    particle_id(&format!("minecraft:{name}"))
}

fn item_particle(name: &str) -> Particle {
    let item = mcr_data::v26_3()
        .item_by_name(&format!("minecraft:{name}"))
        .map_or(0, |i| i as i32);
    Particle::Item(ItemStackTemplate {
        item,
        count: 1,
        components: Default::default(),
    })
}

/// What the handler reads and writes for one event.
pub struct Cx<'a> {
    pub state: &'a ClientState,
    pub effects: &'a mut WorldEffects,
    pub audio: Option<&'a mut Audio>,
    pub camera: [f64; 3],
    /// The current event plays its sounds with `distanceDelay`.
    pub delay: bool,
}

/// `LevelEventHandler#levelEvent`: the trial spawner, vault and cobweb events play their sounds
/// with `distanceDelay`.
pub fn delays_sound(event: i32) -> bool {
    (3012..=3021).contains(&event)
}

impl Cx<'_> {
    fn sound(
        &mut self,
        name: &str,
        at: Option<[f64; 3]>,
        volume: f32,
        pitch: f32,
        source: SoundSource,
    ) {
        let delay = match at {
            Some(at) if self.delay => crate::audio::distance_delay_ticks(self.camera, at),
            _ => 0,
        };
        let Some(audio) = self.audio.as_deref_mut() else {
            return;
        };
        match at {
            Some(at) if delay > 0 => audio.play_delayed(name, at, volume, pitch, source, delay),
            _ => {
                audio.play(name, at, volume, pitch, source);
            }
        }
    }

    fn add(&mut self, particle: &Particle, at: [f64; 3], vel: [f64; 3]) {
        self.effects.add_particle(particle, at, vel);
    }

    fn state_at(&self, pos: BlockPos) -> u32 {
        self.state
            .chunks
            .block(pos.x, pos.y, pos.z)
            .map_or(0, |b| b.0)
    }

    fn name_at(&self, pos: BlockPos) -> &'static str {
        let data = mcr_data::v26_3();
        data.state_block_name(self.state_at(pos))
            .map_or("", |n| n.strip_prefix("minecraft:").unwrap_or(n))
    }

    fn is_air(&self, pos: BlockPos) -> bool {
        mcr_data::v26_3()
            .block_state(self.state_at(pos))
            .is_none_or(|i| i.is_air())
    }

    /// Top of the block's outline at its centre column, `VoxelShape#max(Y, 0.5, 0.5)`.
    fn shape_top(&self, pos: BlockPos) -> f64 {
        mcr_data::v26_3()
            .outline_shape(self.state_at(pos))
            .iter()
            .filter(|b| b.min[0] <= 0.5 && b.max[0] >= 0.5 && b.min[2] <= 0.5 && b.max[2] >= 0.5)
            .map(|b| f64::from(b.max[1]))
            .fold(0.0, f64::max)
    }

    fn shape_height(&self, pos: BlockPos) -> f64 {
        mcr_data::v26_3()
            .outline_shape(self.state_at(pos))
            .iter()
            .map(|b| f64::from(b.max[1]))
            .fold(0.0, f64::max)
    }
}

#[derive(Debug)]
pub struct LevelEvents {
    rng: Rng,
}

impl Default for LevelEvents {
    fn default() -> Self {
        Self {
            rng: Rng::new(0x1E7E_1E7E_5EED),
        }
    }
}

impl LevelEvents {
    pub fn handle(&mut self, cx: &mut Cx<'_>, event: i32, pos: BlockPos, data: i32, global: bool) {
        if global {
            self.global(cx, event, pos);
            return;
        }
        let Some(row) = lookup(event) else {
            return;
        };
        cx.delay = delays_sound(event);
        if let Some(s) = row.sound {
            let pitch = self.rng.pitch(s.pitch);
            cx.sound(s.name, Some(center(pos)), s.volume, pitch, s.source);
        }
        self.fx(cx, row.fx, pos, data);
        cx.delay = false;
    }

    fn global(&mut self, cx: &mut Cx<'_>, event: i32, pos: BlockPos) {
        let Some(&(_, name, volume)) = GLOBAL_EVENTS.iter().find(|(id, ..)| *id == event) else {
            return;
        };
        let c = center(pos);
        let d: [f64; 3] = std::array::from_fn(|i| c[i] - cx.camera[i]);
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        let at = if len > 1.0e-4 {
            std::array::from_fn(|i| cx.camera[i] + d[i] / len * 2.0)
        } else {
            cx.camera
        };
        cx.sound(name, Some(at), volume, 1.0, SoundSource::Hostile);
    }

    fn fx(&mut self, cx: &mut Cx<'_>, fx: Fx, pos: BlockPos, data: i32) {
        let r = &mut self.rng;
        let o = corner(pos);
        match fx {
            Fx::None | Fx::Jukebox => {}
            Fx::Extinguish => {
                let (name, volume, pitch) = match data {
                    0 => ("block.fire.extinguish", 0.5, r.pitch(Pitch::Hiss(2.6, 0.8))),
                    1 => (
                        "entity.generic.extinguish_fire",
                        0.7,
                        r.pitch(Pitch::Hiss(1.6, 0.4)),
                    ),
                    _ => return,
                };
                cx.sound(name, Some(center(pos)), volume, pitch, SoundSource::Blocks);
            }
            Fx::PortalTravel => {
                let pitch = r.f32() * 0.4 + 0.8;
                cx.sound(
                    "block.portal.travel",
                    None,
                    0.25,
                    pitch,
                    SoundSource::Ambient,
                );
            }
            Fx::Composter => {
                let name = if data > 0 {
                    "block.composter.fill_success"
                } else {
                    "block.composter.fill"
                };
                cx.sound(name, Some(center(pos)), 1.0, 1.0, SoundSource::Blocks);
                let top = cx.shape_top(pos) + 0.03125;
                let p = simple("composter");
                for _ in 0..10 {
                    let v = [
                        r.gaussian() * 0.02,
                        r.gaussian() * 0.02,
                        r.gaussian() * 0.02,
                    ];
                    let at = [
                        o[0] + 0.1875 + 0.625 * f64::from(r.f32()),
                        o[1] + top + f64::from(r.f32()) * (1.0 - top),
                        o[2] + 0.1875 + 0.625 * f64::from(r.f32()),
                    ];
                    cx.add(&p, at, v);
                }
            }
            Fx::LavaFizz => {
                let p = simple("large_smoke");
                for _ in 0..8 {
                    cx.add(&p, [o[0] + r.f64(), o[1] + 1.2, o[2] + r.f64()], [0.0; 3]);
                }
            }
            Fx::TorchBurnout => {
                let p = simple("smoke");
                for _ in 0..5 {
                    let at = o.map(|v| v + r.f64() * 0.6 + 0.2);
                    cx.add(&p, at, [0.0; 3]);
                }
            }
            Fx::PortalFrameFill => {
                let p = simple("smoke");
                for _ in 0..16 {
                    let at = [
                        o[0] + (5.0 + r.f64() * 6.0) / 16.0,
                        o[1] + 0.8125,
                        o[2] + (5.0 + r.f64() * 6.0) / 16.0,
                    ];
                    cx.add(&p, at, [0.0; 3]);
                }
            }
            Fx::Drip => {
                if cx.name_at(pos) == "pointed_dripstone" {
                    let p = simple("dripping_dripstone_water");
                    cx.add(&p, [o[0] + 0.5, o[1] + 0.625, o[2] + 0.5], [0.0; 3]);
                }
            }
            Fx::Growth => self.growth(cx, pos, data.clamp(0, 64)),
            Fx::Shoot(name) => self.shoot(cx, pos, data, &simple(name)),
            Fx::DestroyBlock => {
                if let Ok(state) = u32::try_from(data)
                    && let Some(audio) = cx.audio.as_deref_mut()
                    && mcr_data::v26_3()
                        .block_state(state)
                        .is_some_and(|i| !i.is_air())
                {
                    audio.play_block(state, BlockSound::Break, center(pos));
                }
                cx.effects.block_break(cx.state, pos, data);
            }
            Fx::PotionSplash { instant } => self.potion(cx, pos, data, instant),
            Fx::EyeOfEnder => {
                let c = [o[0] + 0.5, o[1], o[2] + 0.5];
                let item = item_particle("ender_eye");
                for _ in 0..8 {
                    let v = [r.gaussian() * 0.15, r.f64() * 0.2, r.gaussian() * 0.15];
                    cx.add(&item, c, v);
                }
                let portal = simple("portal");
                for i in 0..40 {
                    let a = f64::from(i) * std::f64::consts::PI / 20.0;
                    let at = [c[0] + a.cos() * 5.0, c[1] - 0.4, c[2] + a.sin() * 5.0];
                    cx.add(&portal, at, [a.cos() * -5.0, 0.0, a.sin() * -5.0]);
                    cx.add(&portal, at, [a.cos() * -7.0, 0.0, a.sin() * -7.0]);
                }
            }
            Fx::MobSpawn => {
                let (smoke, flame) = (simple("smoke"), simple("flame"));
                for _ in 0..20 {
                    let at = o.map(|v| v + 0.5 + (r.f64() - 0.5) * 2.0);
                    cx.add(&smoke, at, [0.0; 3]);
                    cx.add(&flame, at, [0.0; 3]);
                }
            }
            Fx::DragonBreath => {
                for _ in 0..200 {
                    let dist = r.f32() * 4.0;
                    let angle = r.f32() * std::f32::consts::TAU;
                    let v = [
                        f64::from(angle.cos() * dist),
                        0.01 + r.f64() * 0.5,
                        f64::from(angle.sin() * dist),
                    ];
                    let at = [o[0] + v[0] * 0.1, o[1] + 0.3, o[2] + v[2] * 0.1];
                    cx.add(&Particle::DragonBreath { power: dist }, at, v);
                }
                if data == 1 {
                    let pitch = r.pitch(Pitch::Narrow);
                    let name = "entity.dragon_fireball.explode";
                    cx.sound(name, Some(center(pos)), 1.0, pitch, SoundSource::Hostile);
                }
            }
            Fx::Explosion => cx.add(&simple("explosion"), center(pos), [0.0; 3]),
            Fx::Evaporate => {
                let p = simple("cloud");
                for _ in 0..8 {
                    cx.add(&p, [o[0] + r.f64(), o[1] + 1.2, o[2] + r.f64()], [0.0; 3]);
                }
            }
            Fx::InBlock(name) => {
                let height = if cx.is_air(pos) {
                    1.0
                } else {
                    cx.shape_height(pos)
                };
                self.spread(cx, pos, data.clamp(0, 64), 0.5, height, true, &simple(name));
            }
            Fx::Smash => {
                let p = Particle::BlockState {
                    kind: BlockParticleKind::DustPillar,
                    state: cx.state_at(pos) as i32,
                };
                let c = [o[0] + 0.5, o[1] + 1.0, o[2] + 0.5];
                let count = data.clamp(0, 300) as f32;
                let mut i = 0;
                while (i as f32) < count / 3.0 {
                    let at = [c[0] + r.gaussian() / 2.0, c[1], c[2] + r.gaussian() / 2.0];
                    let v = [r.gaussian() * 0.2, r.gaussian() * 0.2, r.gaussian() * 0.2];
                    cx.add(&p, at, v);
                    i += 1;
                }
                let mut i = 0;
                while (i as f32) < count / 1.5 {
                    let a = f64::from(i);
                    let at = [
                        c[0] + 3.5 * a.cos() + r.gaussian() / 2.0,
                        c[1],
                        c[2] + 3.5 * a.sin() + r.gaussian() / 2.0,
                    ];
                    let v = [
                        r.gaussian() * 0.05,
                        r.gaussian() * 0.05,
                        r.gaussian() * 0.05,
                    ];
                    cx.add(&p, at, v);
                    i += 1;
                }
            }
            Fx::BlockParticles => cx.effects.block_break(cx.state, pos, data),
            Fx::Teleport(radius) => {
                let to = unpack_offset(pos, data, radius);
                let p = simple("portal");
                for _ in 0..128 {
                    let t = r.f64();
                    let v = [
                        f64::from((r.f32() - 0.5) * 0.2),
                        f64::from((r.f32() - 0.5) * 0.2),
                        f64::from((r.f32() - 0.5) * 0.2),
                    ];
                    let at = [
                        lerp(t, to[0], o[0]) + (r.f64() - 0.5) + 0.5,
                        lerp(t, to[1], o[1]) + r.f64() - 0.5,
                        lerp(t, to[2], o[2]) + (r.f64() - 0.5) + 0.5,
                    ];
                    cx.add(&p, at, v);
                }
            }
            Fx::EndermanTeleport => {
                let to = unpack_offset(pos, data, [127, 127, 127]);
                let (w, h) = ENDERMAN_SIZE;
                let p = simple("portal");
                for _ in 0..128 {
                    let t = r.f64();
                    let v = [
                        f64::from((r.f32() - 0.5) * 0.2),
                        f64::from((r.f32() - 0.5) * 0.2),
                        f64::from((r.f32() - 0.5) * 0.2),
                    ];
                    let at = [
                        lerp(t, to[0], o[0]) + (r.f64() - 0.5) * w * 2.0,
                        lerp(t, to[1], o[1]) + r.f64() * h,
                        lerp(t, to[2], o[2]) + (r.f64() - 0.5) * w * 2.0,
                    ];
                    cx.add(&p, at, v);
                }
            }
            Fx::Breaking { sound } => {
                let state = cx.state_at(pos);
                let air = mcr_data::v26_3()
                    .block_state(state)
                    .is_none_or(|i| i.is_air());
                if air {
                    return;
                }
                if sound && let Some(audio) = cx.audio.as_deref_mut() {
                    audio.play_block(state, BlockSound::Hit, center(pos));
                }
                let face = data.clamp(0, 5) as u8;
                cx.effects.block_crack(cx.state, pos, face, state as i32);
            }
            Fx::GatewaySpawn => {
                let p = simple("explosion_emitter");
                cx.effects
                    .add_particle_vanilla(&p, center(pos), [0.0; 3], true);
            }
            Fx::Spark => {
                let p = simple("electric_spark");
                if (0..3).contains(&data) {
                    let c = center(pos);
                    for _ in 0..r.uniform(10, 19) {
                        let at: [f64; 3] = std::array::from_fn(|a| {
                            c[a] + r.between(-1.0, 1.0)
                                * if a == data as usize { 0.5 } else { 0.125 }
                        });
                        let v: [f64; 3] = std::array::from_fn(|a| {
                            if a == data as usize {
                                r.between(-1.0, 1.0)
                            } else {
                                0.0
                            }
                        });
                        cx.add(&p, at, v);
                    }
                } else {
                    self.faces(cx, pos, &p, 3, 5);
                }
            }
            Fx::Faces(name, lo, hi) => self.faces(cx, pos, &simple(name), lo, hi),
            Fx::SculkCharge => self.sculk(cx, pos, data),
            Fx::Shriek => {
                for i in 0..10 {
                    let at = [o[0] + 0.5, o[1] + SHRIEKER_TOP, o[2] + 0.5];
                    cx.add(&Particle::Shriek { delay: i * 5 }, at, [0.0; 3]);
                }
                let data = mcr_data::v26_3();
                let waterlogged = data
                    .state_properties(cx.state_at(pos))
                    .any(|p| p == ("waterlogged", "true"));
                if !waterlogged {
                    let pitch = 0.6 + self.rng.f32() * 0.4;
                    let at = [o[0] + 0.5, o[1] + SHRIEKER_TOP, o[2] + 0.5];
                    cx.sound(
                        "block.sculk_shrieker.shriek",
                        Some(at),
                        2.0,
                        pitch,
                        SoundSource::Blocks,
                    );
                }
            }
            Fx::Brush => {
                let block = u32::try_from(data)
                    .ok()
                    .and_then(|s| mcr_data::v26_3().state_block_name(s));
                let sound = match block {
                    Some("minecraft:suspicious_sand") => Some("item.brush.brushing.sand.complete"),
                    Some("minecraft:suspicious_gravel") => {
                        Some("item.brush.brushing.gravel.complete")
                    }
                    _ => None,
                };
                if let Some(name) = sound {
                    cx.sound(name, Some(center(pos)), 1.0, 1.0, SoundSource::Players);
                }
                cx.effects.block_break(cx.state, pos, data);
            }
            Fx::TrialSpawn => {
                let flame = simple(if data == 1 {
                    "soul_fire_flame"
                } else {
                    "flame"
                });
                let smoke = simple("smoke");
                for _ in 0..20 {
                    let at = o.map(|v| v + 0.5 + (r.f64() - 0.5) * 2.0);
                    cx.add(&smoke, at, [0.0; 3]);
                    cx.add(&flame, at, [0.0; 3]);
                }
            }
            Fx::Detect(name) => self.detect(cx, pos, data.clamp(0, 10), &simple(name)),
            Fx::Eject => {
                let (flame, smoke) = (simple("small_flame"), simple("smoke"));
                for _ in 0..20 {
                    let at = o.map(|v| v + 0.4 + r.f64() * 0.2);
                    let v = [
                        r.gaussian() * 0.02,
                        r.gaussian() * 0.02,
                        r.gaussian() * 0.02,
                    ];
                    cx.add(&flame, at, [v[0], v[1], v[2] * 0.25]);
                    cx.add(&smoke, at, v);
                }
            }
            Fx::VaultActivate => {
                if cx.name_at(pos) != "vault" {
                    return;
                }
                let flame = simple(if data == 0 {
                    "small_flame"
                } else {
                    "soul_fire_flame"
                });
                let smoke = simple("smoke");
                for _ in 0..20 {
                    let at = [
                        o[0] + r.between(0.1, 0.9),
                        o[1] + r.between(0.25, 0.75),
                        o[2] + r.between(0.1, 0.9),
                    ];
                    cx.add(&smoke, at, [0.0; 3]);
                    cx.add(&flame, at, [0.0; 3]);
                }
                let pitch = r.pitch(Spread);
                cx.sound(
                    "block.vault.activate",
                    Some(center(pos)),
                    1.0,
                    pitch,
                    SoundSource::Blocks,
                );
            }
            Fx::VaultDeactivate => {
                let flame = simple(if data == 0 {
                    "small_flame"
                } else {
                    "soul_fire_flame"
                });
                for _ in 0..20 {
                    let at = o.map(|v| v + r.between(0.4, 0.6));
                    let v = [
                        r.gaussian() * 0.02,
                        r.gaussian() * 0.02,
                        r.gaussian() * 0.02,
                    ];
                    cx.add(&flame, at, v);
                }
            }
            Fx::Cobweb => {
                let p = simple("poof");
                for _ in 0..10 {
                    let v = [
                        r.gaussian() * 0.02,
                        r.gaussian() * 0.02,
                        r.gaussian() * 0.02,
                    ];
                    let at = o.map(|c| c + r.f64());
                    cx.add(&p, at, v);
                }
            }
            Fx::BecomeOminous => {
                let volume = if data == 0 { 0.3 } else { 1.0 };
                let pitch = r.pitch(Spread);
                let name = "block.trial_spawner.ominous_activate";
                cx.sound(name, Some(center(pos)), volume, pitch, SoundSource::Blocks);
                self.detect(cx, pos, 0, &simple("trial_spawner_detection_ominous"));
                let (omen, soul) = (simple("trial_omen"), simple("soul_fire_flame"));
                let r = &mut self.rng;
                for _ in 0..20 {
                    let at = o.map(|v| v + 0.5 + (r.f64() - 0.5) * 2.0);
                    let v = [
                        r.gaussian() * 0.02,
                        r.gaussian() * 0.02,
                        r.gaussian() * 0.02,
                    ];
                    cx.add(&omen, at, v);
                    cx.add(&soul, at, v);
                }
            }
        }
    }

    /// `ParticleUtils#spawnParticlesOnBlockFaces`.
    fn faces(&mut self, cx: &mut Cx<'_>, pos: BlockPos, p: &Particle, lo: i32, hi: i32) {
        let r = &mut self.rng;
        let c = center(pos);
        for step in STEPS {
            for _ in 0..r.uniform(lo, hi) {
                let speed = [
                    r.between(-0.5, 0.5),
                    r.between(-0.5, 0.5),
                    r.between(-0.5, 0.5),
                ];
                let (at, v) = on_face(r, c, step, 0.55, speed);
                cx.add(p, at, v);
            }
        }
    }

    /// `TrialSpawner#addDetectPlayerParticles`.
    fn detect(&mut self, cx: &mut Cx<'_>, pos: BlockPos, extra: i32, p: &Particle) {
        let r = &mut self.rng;
        let o = corner(pos);
        for _ in 0..30 + extra * 5 {
            let sx = f64::from(2.0 * r.f32() - 1.0) * 0.65;
            let sz = f64::from(2.0 * r.f32() - 1.0) * 0.65;
            let at = [
                o[0] + 0.5 + sx,
                o[1] + 0.1 + f64::from(r.f32()) * 0.8,
                o[2] + 0.5 + sz,
            ];
            cx.add(p, at, [0.0; 3]);
        }
    }

    /// `BoneMealItem#addGrowthParticles`.
    fn growth(&mut self, cx: &mut Cx<'_>, pos: BlockPos, count: i32) {
        let p = simple("happy_villager");
        let above = BlockPos::new(pos.x, pos.y + 1, pos.z);
        match cx.name_at(pos) {
            "grass_block" | "netherrack" | "crimson_nylium" | "warped_nylium" | "moss_block"
            | "pale_moss_block" => self.spread(cx, above, count * 3, 3.0, 1.0, false, &p),
            "water" => self.spread(cx, pos, count * 3, 3.0, 1.0, false, &p),
            "air" | "cave_air" | "void_air" | "" => {}
            _ => {
                let height = cx.shape_height(pos);
                self.spread(cx, pos, count, 0.5, height, true, &p);
            }
        }
    }

    /// `ParticleUtils#spawnParticles`.
    #[allow(clippy::too_many_arguments)]
    fn spread(
        &mut self,
        cx: &mut Cx<'_>,
        pos: BlockPos,
        count: i32,
        width: f64,
        height: f64,
        floating: bool,
        p: &Particle,
    ) {
        let r = &mut self.rng;
        let o = corner(pos);
        for _ in 0..count {
            let v = [
                r.gaussian() * 0.02,
                r.gaussian() * 0.02,
                r.gaussian() * 0.02,
            ];
            let start = 0.5 - width;
            let at = [
                o[0] + start + r.f64() * width * 2.0,
                o[1] + r.f64() * height,
                o[2] + start + r.f64() * width * 2.0,
            ];
            let below = BlockPos::new(
                at[0].floor() as i32,
                at[1].floor() as i32 - 1,
                at[2].floor() as i32,
            );
            if floating || !cx.is_air(below) {
                cx.add(p, at, v);
            }
        }
    }

    /// `LevelEventHandler#shootParticles`: a puff out of a dispenser's face.
    fn shoot(&mut self, cx: &mut Cx<'_>, pos: BlockPos, data: i32, p: &Particle) {
        let r = &mut self.rng;
        let [nx, ny, nz] = STEPS[data.rem_euclid(6) as usize].map(f64::from);
        let o = corner(pos);
        for _ in 0..10 {
            let pow = r.f64() * 0.2 + 0.01;
            let at = [
                o[0] + nx * 0.6 + 0.5 + nx * 0.01 + (r.f64() - 0.5) * nz * 0.5,
                o[1] + ny * 0.6 + 0.5 + ny * 0.01 + (r.f64() - 0.5) * ny * 0.5,
                o[2] + nz * 0.6 + 0.5 + nz * 0.01 + (r.f64() - 0.5) * nx * 0.5,
            ];
            let v = [
                nx * pow + r.gaussian() * 0.01,
                ny * pow + r.gaussian() * 0.01,
                nz * pow + r.gaussian() * 0.01,
            ];
            cx.add(p, at, v);
        }
    }

    /// `LevelEventHandler#potionSplashParticles`.
    fn potion(&mut self, cx: &mut Cx<'_>, pos: BlockPos, data: i32, instant: bool) {
        let r = &mut self.rng;
        let at = [
            f64::from(pos.x) + 0.5,
            f64::from(pos.y),
            f64::from(pos.z) + 0.5,
        ];
        let item = item_particle("splash_potion");
        for _ in 0..8 {
            let v = [r.gaussian() * 0.15, r.f64() * 0.2, r.gaussian() * 0.15];
            cx.add(&item, at, v);
        }
        let channel = |shift: i32| ((data >> shift) & 0xFF) as f32 / 255.0;
        let (red, green, blue) = (channel(16), channel(8), channel(0));
        let kind = if instant {
            SpellParticleKind::InstantEffect
        } else {
            SpellParticleKind::Effect
        };
        for _ in 0..100 {
            let dist = r.f64() * 4.0;
            let angle = r.f64() * std::f64::consts::TAU;
            let v = [angle.cos() * dist, 0.01 + r.f64() * 0.5, angle.sin() * dist];
            let bright = 0.75 + r.f32() * 0.25;
            let byte = |c: f32| ((c * bright).clamp(0.0, 1.0) * 255.0) as i32;
            let color = (0xFF << 24) | byte(red) << 16 | byte(green) << 8 | byte(blue);
            let p = Particle::Spell {
                kind,
                color,
                power: dist as f32,
            };
            cx.add(&p, [at[0] + v[0] * 0.1, at[1] + 0.3, at[2] + v[2] * 0.1], v);
        }
    }

    /// `LevelEventHandler` case 3006: sculk spreading charge, or the pop when it settles.
    fn sculk(&mut self, cx: &mut Cx<'_>, pos: BlockPos, data: i32) {
        let r = &mut self.rng;
        let count = (data >> 6).clamp(0, 64);
        let c = center(pos);
        if count > 0 {
            if r.f32() < 0.3 + count as f32 * 0.1 {
                let volume = 0.15 + 0.02 * (count * count) as f32 * r.f32();
                let pitch = 0.4 + 0.3 * count as f32 * r.f32();
                cx.sound(
                    "block.sculk.charge",
                    Some(c),
                    volume,
                    pitch,
                    SoundSource::Blocks,
                );
            }
            let faces = (data & 63) as u8;
            for (i, step) in STEPS.iter().enumerate() {
                let (roll, factor) = if faces == 0 {
                    let roll = if i == 0 { std::f32::consts::PI } else { 0.0 };
                    (roll, if step[1] != 0 { 0.65 } else { 0.57 })
                } else if faces & (1 << i) != 0 {
                    (if i == 1 { std::f32::consts::PI } else { 0.0 }, 0.35)
                } else {
                    continue;
                };
                let p = Particle::SculkCharge { roll };
                for _ in 0..r.uniform(0, count) {
                    let speed = [
                        r.between(-0.005, 0.005),
                        r.between(-0.005, 0.005),
                        r.between(-0.005, 0.005),
                    ];
                    let (at, v) = on_face(r, c, *step, factor, speed);
                    cx.add(&p, at, v);
                }
            }
        } else {
            cx.sound("block.sculk.charge", Some(c), 1.0, 1.0, SoundSource::Blocks);
            let solid = mcr_data::v26_3()
                .block_state(cx.state_at(pos))
                .is_some_and(|i| i.is_full_collision());
            let (n, spread) = if solid { (40, 0.45) } else { (20, 0.25) };
            let p = simple("sculk_charge_pop");
            for _ in 0..n {
                let v = [
                    f64::from(2.0 * r.f32() - 1.0),
                    f64::from(2.0 * r.f32() - 1.0),
                    f64::from(2.0 * r.f32() - 1.0),
                ];
                let at: [f64; 3] = std::array::from_fn(|a| c[a] + v[a] * spread);
                cx.add(&p, at, v.map(|x| x * 0.07));
            }
        }
    }
}

fn lerp(t: f64, a: f64, b: f64) -> f64 {
    a + t * (b - a)
}

/// `BlockUtil#unpackDifferenceInPosition`, as a block corner.
fn unpack_offset(pos: BlockPos, data: i32, radius: [i32; 3]) -> [f64; 3] {
    let d = [
        ((data >> 16) & 0xFF) - radius[0],
        ((data >> 8) & 0xFF) - radius[1],
        (data & 0xFF) - radius[2],
    ];
    [
        f64::from(pos.x) + f64::from(d[0]),
        f64::from(pos.y) + f64::from(d[1]),
        f64::from(pos.z) + f64::from(d[2]),
    ]
}

/// `ParticleUtils#spawnParticleOnFace`.
fn on_face(
    r: &mut Rng,
    c: [f64; 3],
    step: [i32; 3],
    factor: f64,
    speed: [f64; 3],
) -> ([f64; 3], [f64; 3]) {
    let mut at = [0.0; 3];
    let mut v = [0.0; 3];
    for a in 0..3 {
        if step[a] == 0 {
            at[a] = c[a] + r.between(-0.5, 0.5);
            v[a] = speed[a];
        } else {
            at[a] = c[a] + f64::from(step[a]) * factor;
        }
    }
    (at, v)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every id `LevelEventHandler#levelEvent` handles.
    const VANILLA_IDS: &[i32] = &[
        1000, 1001, 1002, 1004, 1009, 1010, 1011, 1015, 1016, 1017, 1018, 1019, 1020, 1021, 1022,
        1024, 1025, 1026, 1027, 1029, 1030, 1031, 1032, 1033, 1034, 1035, 1039, 1040, 1041, 1042,
        1043, 1044, 1045, 1046, 1047, 1048, 1049, 1050, 1051, 1052, 1053, 1054, 1500, 1501, 1502,
        1503, 1504, 1505, 2000, 2001, 2002, 2003, 2004, 2006, 2007, 2008, 2009, 2010, 2011, 2012,
        2013, 2014, 2015, 2016, 2017, 2018, 2019, 2020, 3000, 3001, 3002, 3003, 3004, 3005, 3006,
        3007, 3008, 3009, 3011, 3012, 3013, 3014, 3015, 3016, 3017, 3018, 3019, 3020, 3021,
    ];

    #[test]
    fn only_the_trial_spawner_vault_and_cobweb_events_delay_their_sounds() {
        assert!((3012..=3021).all(delays_sound));
        assert!(
            ![3000, 3009, 3011, 2001, 1000, 3022]
                .into_iter()
                .any(delays_sound)
        );
    }

    #[test]
    fn delay_grows_with_distance_and_near_sounds_play_at_once() {
        use crate::audio::distance_delay_ticks as ticks;
        let eye = [0.0, 64.0, 0.0];
        assert_eq!(
            ticks(eye, [10.0, 64.0, 0.0]),
            0,
            "ten blocks is not beyond the threshold"
        );
        assert_eq!(ticks(eye, [20.0, 64.0, 0.0]), 10);
        assert_eq!(ticks(eye, [100.0, 64.0, 0.0]), 50);
        assert_eq!(ticks(eye, [0.0, 64.0, -41.0]), 20, "20.5 ticks truncate");
        let mut last = 0;
        for d in (11..200).step_by(7) {
            let t = ticks(eye, [f64::from(d), 64.0, 0.0]);
            assert!(t >= last);
            last = t;
        }
        assert_eq!(ticks(eye, [f64::NAN, 0.0, 0.0]), 0);
    }

    #[test]
    fn table_covers_every_vanilla_event_once_in_order() {
        let ids: Vec<i32> = EVENTS.iter().map(|r| r.id).collect();
        assert_eq!(ids, VANILLA_IDS);
        for id in VANILLA_IDS {
            assert_eq!(lookup(*id).map(|r| r.id), Some(*id));
        }
        assert!(lookup(1003).is_none() && lookup(-1).is_none() && lookup(i32::MAX).is_none());
    }

    #[test]
    fn every_row_makes_a_sound_or_runs_a_routine() {
        for r in EVENTS {
            assert!(r.sound.is_some() || r.fx != Fx::None, "{}", r.id);
        }
    }

    #[test]
    fn rows_map_to_vanilla_sounds_and_particles() {
        let sound = |id| {
            lookup(id)
                .and_then(|r| r.sound)
                .map(|s| (s.name, s.source, s.volume))
        };
        let particle = |id| lookup(id).and_then(|r| r.fx.particle());
        assert_eq!(sound(1000), Some(("block.dispenser.dispense", B, 1.0)));
        assert_eq!(
            sound(1019),
            Some(("entity.zombie.attack_wooden_door", H, 2.0))
        );
        assert_eq!(sound(1030), Some(("block.anvil.use", B, 1.0)));
        assert_eq!(sound(1049), Some(("block.crafter.craft", B, 1.0)));
        assert_eq!(sound(1042), Some(("block.grindstone.use", B, 1.0)));
        assert_eq!(sound(1505), Some(("item.bone_meal.use", B, 1.0)));
        assert_eq!(sound(3001), Some(("entity.ender_dragon.growl", H, 64.0)));
        assert_eq!(particle(1505), Some("happy_villager"));
        assert_eq!(particle(2000), Some("smoke"));
        assert_eq!(particle(2002), Some("effect"));
        assert_eq!(particle(2007), Some("instant_effect"));
        assert_eq!(particle(2003), Some("portal"));
        assert_eq!(particle(3003), Some("wax_on"));
        assert_eq!(particle(3005), Some("scrape"));
        assert_eq!(particle(3007), Some("shriek"));
        assert_eq!(particle(2020), Some("block"));
        assert_eq!(lookup(1010).unwrap().fx, Fx::Jukebox);
    }

    #[test]
    fn every_name_resolves_in_the_registries() {
        let data = mcr_data::v26_3();
        let sounds = data.registry("minecraft:sound_event").unwrap();
        let particles = data.registry("minecraft:particle_type").unwrap();
        for r in EVENTS {
            if let Some(s) = r.sound {
                assert!(
                    sounds.id_of(&format!("minecraft:{}", s.name)).is_some(),
                    "{}",
                    s.name
                );
            }
            if let Some(p) = r.fx.particle() {
                assert!(particles.id_of(&format!("minecraft:{p}")).is_some(), "{p}");
            }
        }
        for (_, name, _) in GLOBAL_EVENTS {
            assert!(
                sounds.id_of(&format!("minecraft:{name}")).is_some(),
                "{name}"
            );
        }
    }

    #[test]
    fn pitches_stay_in_vanilla_ranges() {
        let mut r = Rng::new(7);
        for _ in 0..1000 {
            let p = r.pitch(Pitch::Spread);
            assert!((0.8..=1.2).contains(&p));
            let n = r.pitch(Narrow);
            assert!((0.9..=1.0).contains(&n));
            let t = r.pitch(Pitch::Throw);
            assert!((0.4 / 1.2..=0.5).contains(&t));
            assert!((3..=5).contains(&r.uniform(3, 5)));
        }
    }

    #[test]
    fn unpacked_offsets_match_vanilla() {
        let pos = BlockPos::new(10, 64, -3);
        let packed = (16 + 2) << 16 | (8 - 1) << 8 | (16 - 5);
        assert_eq!(unpack_offset(pos, packed, [16, 8, 16]), [12.0, 63.0, -8.0]);
    }
}
