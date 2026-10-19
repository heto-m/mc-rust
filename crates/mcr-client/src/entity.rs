use mcr_data::Version;
use mcr_physics::{Body, BodyFlags, BodyKind, BodyScratch, CollisionWorld, PhysicsData};
use mcr_protocol::play::clientbound::entity::{PositionPath, VecDelta};
use mcr_protocol::types::ItemStack;
use mcr_protocol::types::component::{AttributeModifier, EquipmentSlot};
use mcr_protocol::types::entity_data::EntityDataValue;
use rustc_hash::FxHashMap;
use uuid::Uuid;

const NO_GRAVITY: u8 = 5;
const ARROW_FLAGS: u8 = 8;
const ARROW_NO_PHYSICS: i8 = 2;
const TRIDENT_LOYALTY: u8 = 11;
const WITHER_SKULL_DANGEROUS: u8 = 8;
const FIREWORK_ATTACHED: u8 = 9;
const FIREWORK_SHOT_AT_ANGLE: u8 = 10;
const TNT_FUSE: u8 = 8;
const TNT_DEFAULT_FUSE: i32 = 80;
pub const MINECART_TNT_FUSE: i32 = 80;

pub const MAX_ENTITIES: usize = 1 << 16;
const MAX_SIMULATED_PER_TICK: usize = 4096;
const MAX_PASSENGERS: usize = 4096;
const MAX_EFFECTS: usize = 64;
const MAX_ATTRIBUTES: usize = 128;
/// Steps a stepped interpolation keeps queued; older ones are consumed when more arrive.
const MAX_QUEUED_STEPS: usize = 32;
/// Bounds a server-sent step duration.
const MAX_STEP_TICKS: i32 = 1200;
const CREEPER_SWELL_DIR: u8 = 16;
const CREEPER_IGNITED: u8 = 18;
const TOO_BIG_TO_INTERPOLATE_SQR: f64 = 4096.0;
const DELTA_SCALE: f64 = 4096.0;

/// Entity types the client simulates between server updates, as vanilla's `SERVER_AND_CLIENT` movers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SimShape {
    pub kind: BodyKind,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Default)]
pub struct SimKinds {
    by_type: FxHashMap<i32, SimShape>,
    pub player_type: i32,
    pub creeper_type: Option<i32>,
    pub dragon_type: Option<i32>,
    pub knot_type: Option<i32>,
}

impl SimKinds {
    pub fn resolve(data: &Version) -> Self {
        const NAMES: [(&str, BodyKind); 25] = [
            ("item", BodyKind::Item),
            ("experience_orb", BodyKind::ExperienceOrb),
            ("tnt", BodyKind::Tnt),
            ("falling_block", BodyKind::FallingBlock),
            ("snowball", BodyKind::Thrown),
            ("egg", BodyKind::Thrown),
            ("blue_egg", BodyKind::Thrown),
            ("brown_egg", BodyKind::Thrown),
            ("ender_pearl", BodyKind::Thrown),
            ("experience_bottle", BodyKind::ExperienceBottle),
            ("splash_potion", BodyKind::Potion),
            ("lingering_potion", BodyKind::Potion),
            ("arrow", BodyKind::Arrow),
            ("spectral_arrow", BodyKind::Arrow),
            ("trident", BodyKind::Trident),
            ("fireball", BodyKind::Fireball),
            ("small_fireball", BodyKind::Fireball),
            ("dragon_fireball", BodyKind::Fireball),
            ("wither_skull", BodyKind::WitherSkull),
            ("wind_charge", BodyKind::WindCharge),
            ("breeze_wind_charge", BodyKind::WindCharge),
            ("llama_spit", BodyKind::LlamaSpit),
            ("firework_rocket", BodyKind::FireworkRocket),
            ("eye_of_ender", BodyKind::Drift),
            ("shulker_bullet", BodyKind::Drift),
        ];
        let by_type = NAMES
            .iter()
            .filter_map(|&(name, kind)| {
                let id = data.entity_type_by_name(&format!("minecraft:{name}"))?;
                let info = data.entity_type_info(id)?;
                let shape = SimShape {
                    kind,
                    width: f64::from(info.width),
                    height: f64::from(info.height),
                };
                Some((id as i32, shape))
            })
            .collect();
        let player_type = data
            .entity_type_by_name("minecraft:player")
            .map_or(-1, |id| id as i32);
        let creeper_type = data
            .entity_type_by_name("minecraft:creeper")
            .map(|id| id as i32);
        let dragon_type = data
            .entity_type_by_name("minecraft:ender_dragon")
            .map(|id| id as i32);
        let knot_type = data
            .entity_type_by_name("minecraft:leash_knot")
            .map(|id| id as i32);
        Self {
            by_type,
            player_type,
            creeper_type,
            dragon_type,
            knot_type,
        }
    }

    pub fn get(&self, entity_type: i32) -> Option<SimShape> {
        self.by_type.get(&entity_type).copied()
    }
}

/// How the client moves an entity towards server positions: vanilla's `InterpolationHandler`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lerp {
    /// No handler: positions apply at once, as for simulated and static entities.
    Snap,
    /// `LinearInterpolationHandler` over this many ticks.
    Linear(u8),
    /// `SteppedInterpolationHandler` with the type's update interval.
    Stepped(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
struct Step {
    pos: [f64; 3],
    yaw: f32,
    pitch: f32,
    ticks: i32,
}

/// `SteppedInterpolationHandler.InterpolationData`: the queued path and its playback clock.
#[derive(Debug, Clone, PartialEq)]
struct StepQueue {
    steps: [Step; MAX_QUEUED_STEPS],
    head: usize,
    len: usize,
    last: Step,
    current_ticks: f32,
    remaining_ticks: f32,
    speed: f32,
}

impl Default for StepQueue {
    fn default() -> Self {
        Self {
            steps: [Step::default(); MAX_QUEUED_STEPS],
            head: 0,
            len: 0,
            last: Step::default(),
            current_ticks: 0.0,
            remaining_ticks: 0.0,
            speed: 1.0,
        }
    }
}

impl StepQueue {
    fn front(&self) -> Step {
        self.steps[self.head]
    }

    fn pop(&mut self) {
        self.head = (self.head + 1) % MAX_QUEUED_STEPS;
        self.len -= 1;
    }

    fn push(&mut self, pos: [f64; 3], yaw: f32, pitch: f32, ticks: i32) {
        let ticks = ticks.clamp(0, MAX_STEP_TICKS);
        if self.len == MAX_QUEUED_STEPS {
            let oldest = self.front();
            self.current_ticks = (self.current_ticks - oldest.ticks as f32).max(0.0);
            self.remaining_ticks = (self.remaining_ticks - oldest.ticks as f32).max(0.0);
            self.last = oldest;
            self.pop();
        }
        self.steps[(self.head + self.len) % MAX_QUEUED_STEPS] = Step {
            pos,
            yaw,
            pitch,
            ticks,
        };
        self.len += 1;
        self.remaining_ticks += ticks as f32;
    }

    fn reset(&mut self) {
        self.len = 0;
        self.remaining_ticks = 0.0;
        self.speed = 1.0;
    }

    /// The pose for this tick, consuming the steps already passed; `None` once all are played.
    fn sample(&mut self) -> Option<([f64; 3], f32, f32)> {
        while self.len > 0 {
            let step = self.front();
            if self.current_ticks < step.ticks as f32 {
                let a = self.current_ticks / step.ticks as f32;
                let t = f64::from(a);
                let last = self.last;
                let pos = std::array::from_fn(|i| last.pos[i] + (step.pos[i] - last.pos[i]) * t);
                return Some((
                    pos,
                    lerp_degrees(last.yaw, step.yaw, a),
                    last.pitch + (step.pitch - last.pitch) * a,
                ));
            }
            self.current_ticks -= step.ticks as f32;
            self.last = step;
            self.pop();
        }
        None
    }

    fn advance(&mut self, tick_speed: f32, interval: u8) {
        let interval = f32::from(interval.max(1));
        let target_speed = (self.remaining_ticks / interval).max(1.0);
        self.speed += (target_speed - self.speed) / interval;
        let mut ticks = tick_speed;
        if ticks * self.speed < self.remaining_ticks {
            ticks *= self.speed;
        } else {
            ticks = self.remaining_ticks;
            self.speed = 1.0;
        }
        self.current_ticks += ticks;
        self.remaining_ticks -= ticks;
    }
}

/// Everything entity simulation reads from the world for one tick.
pub(crate) struct SimContext<'a, W: CollisionWorld + ?Sized> {
    pub world: &'a W,
    pub data: &'static PhysicsData,
    pub scratch: &'a mut BodyScratch,
    /// Mid-eye of the local player, which XP orbs home toward.
    pub follow: Option<[f64; 3]>,
    /// Set while the world is frozen: only entities of this type (players) and mounts keep ticking.
    pub frozen_except: Option<i32>,
    pub creeper_type: Option<i32>,
    pub dragon_type: Option<i32>,
    /// `ClientLevel#getRelativeTickSpeed`: stepped interpolation catches up faster above 20 TPS.
    pub tick_speed: f32,
    /// The local player's id, feet and eye height, which loyal tridents and rockets may follow.
    pub local: Option<(i32, [f64; 3], f64)>,
    pub version: &'static Version,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MobEffect {
    pub amplifier: i32,
    pub duration_ticks: i32,
    pub flags: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AttributeState {
    pub base: f64,
    pub modifiers: Vec<AttributeModifier>,
    /// `RangedAttribute` bounds; `None` for attributes this version does not know.
    pub range: Option<(f64, f64)>,
}

/// Vanilla `RangedAttribute` bounds of the 26.3 attributes.
pub fn attribute_range(name: &str) -> Option<(f64, f64)> {
    let short = name.strip_prefix("minecraft:").unwrap_or(name);
    Some(match short {
        "armor" => (0.0, 30.0),
        "armor_toughness" => (0.0, 20.0),
        "attack_knockback" => (0.0, 5.0),
        "attack_speed" | "block_break_speed" | "burning_time" | "flying_speed" => (0.0, 1024.0),
        "mining_efficiency" | "movement_speed" | "oxygen_bonus" => (0.0, 1024.0),
        "air_drag_modifier" | "attack_damage" | "follow_range" | "friction_modifier" => {
            (0.0, 2048.0)
        }
        "max_absorption" | "tempt_range" => (0.0, 2048.0),
        "below_name_distance" | "name_tag_distance" => (0.0, 512.0),
        "block_interaction_range" | "entity_interaction_range" => (0.0, 64.0),
        "bounciness" | "explosion_knockback_resistance" | "movement_efficiency" => (0.0, 1.0),
        "sneaking_speed" | "spawn_reinforcements" | "sweeping_damage_ratio" => (0.0, 1.0),
        "water_movement_efficiency" => (0.0, 1.0),
        "camera_distance" => (0.0, 32.0),
        "fall_damage_multiplier" => (0.0, 100.0),
        "gravity" => (-1.0, 1.0),
        "jump_strength" => (0.0, 32.0),
        "knockback_resistance" => (-2.0, 1.0),
        "luck" | "safe_fall_distance" => (-1024.0, 1024.0),
        "max_health" => (1.0, 1024.0),
        "scale" => (0.0625, 16.0),
        "step_height" => (0.0, 10.0),
        "submerged_mining_speed" => (0.0, 20.0),
        "waypoint_transmit_range" | "waypoint_receive_range" => (0.0, 6.0e7),
        _ => return None,
    })
}

impl AttributeState {
    pub fn new(base: f64, modifiers: Vec<AttributeModifier>) -> Self {
        Self {
            base,
            modifiers,
            range: None,
        }
    }

    /// Vanilla order: flat additions, then summed base multipliers, then each total multiplier;
    /// the result is sanitised like `RangedAttribute#sanitizeValue` (NaN becomes the minimum).
    pub fn value(&self, skip_modifier: Option<&str>) -> f64 {
        use mcr_protocol::types::component::AttributeOperation as Op;
        let mods = || {
            self.modifiers
                .iter()
                .filter(move |m| Some(m.id.as_str()) != skip_modifier)
        };
        let mut value = self.base;
        let mut mul_base = 0.0;
        for m in mods() {
            match m.operation {
                Op::AddValue => value += m.amount,
                Op::AddMultipliedBase => mul_base += m.amount,
                Op::AddMultipliedTotal => {}
            }
        }
        value += value * mul_base;
        for m in mods().filter(|m| m.operation == Op::AddMultipliedTotal) {
            value *= 1.0 + m.amount;
        }
        match self.range {
            Some((lo, _)) if value.is_nan() => lo,
            Some((lo, hi)) => value.clamp(lo, hi),
            None if value.is_nan() => 0.0,
            None => value,
        }
    }
}

/// `DragonFlightHistory`: the last 64 ticks of the dragon's height and yaw.
#[derive(Debug, Clone, PartialEq)]
pub struct FlightHistory {
    samples: [(f64, f32); 64],
    head: Option<usize>,
}

impl Default for FlightHistory {
    fn default() -> Self {
        Self {
            samples: [(0.0, 0.0); 64],
            head: None,
        }
    }
}

impl FlightHistory {
    pub fn record(&mut self, y: f64, yaw: f32) {
        if self.head.is_none() {
            self.samples = [(y, yaw); 64];
        }
        let head = self.head.map_or(0, |h| (h + 1) & 63);
        self.samples[head] = (y, yaw);
        self.head = Some(head);
    }

    fn at(&self, delay: usize) -> (f64, f32) {
        let head = self.head.unwrap_or(0);
        self.samples[head.wrapping_sub(delay) & 63]
    }

    /// `get(delay, partial)`: between the sample `delay + 1` ticks back and `delay` ticks back.
    pub fn get(&self, delay: usize, partial: f32) -> (f64, f32) {
        let (y, yaw) = self.at(delay);
        let (old_y, old_yaw) = self.at(delay + 1);
        let t = f64::from(partial.clamp(0.0, 1.0));
        (
            old_y + (y - old_y) * t,
            lerp_degrees(old_yaw, yaw, t as f32),
        )
    }

    /// Yaw and the pitch lean (degrees) of `EnderDragonRenderer.submit`.
    pub fn lean(&self, partial: f32) -> (f32, f32) {
        let rot2 = (self.get(5, partial).0 - self.get(10, partial).0) as f32;
        (self.get(7, partial).1, (rot2 * 10.0).clamp(-90.0, 90.0))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entity {
    pub id: i32,
    pub uuid: Uuid,
    pub kind: i32,
    /// Interpolated position; trails `target_pos` by up to three ticks.
    pub pos: [f64; 3],
    /// Also the base for the next relative move.
    pub target_pos: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub target_yaw: f32,
    pub target_pitch: f32,
    pub head_yaw: f32,
    head_target: f32,
    head_steps: u8,
    pub velocity: [f64; 3],
    pub on_ground: bool,
    pub object_data: i32,
    pub data: FxHashMap<u8, EntityDataValue>,
    pub equipment: [ItemStack; 8],
    pub passengers: Vec<i32>,
    pub vehicle: Option<i32>,
    pub leash_holder: Option<i32>,
    pub effects: FxHashMap<i32, MobEffect>,
    pub attributes: FxHashMap<i32, AttributeState>,
    pub sim: Option<SimShape>,
    /// `MinecartTNT.fuse`: client-side, primed by entity event 70; -1 while unlit.
    pub fuse: i32,
    /// `Creeper.swell` and `oldSwell`: client-side, driven by the synched fuse direction and ignite flag.
    pub swell: (i32, i32),
    /// `EnderDragon.flightHistory`, kept only for dragons.
    pub flight: Option<Box<FlightHistory>>,
    /// Chosen at the first position update, once the entity's data says whether it is living.
    pub lerp: Option<Lerp>,
    sim_ticks: u32,
    sim_collided: bool,
    interpolation_left: u8,
    steps: Option<Box<StepQueue>>,
}

impl Entity {
    pub fn new(id: i32, uuid: Uuid, kind: i32, pos: [f64; 3]) -> Self {
        Self {
            id,
            uuid,
            kind,
            pos,
            target_pos: pos,
            yaw: 0.0,
            pitch: 0.0,
            target_yaw: 0.0,
            target_pitch: 0.0,
            head_yaw: 0.0,
            head_target: 0.0,
            head_steps: 0,
            velocity: [0.0; 3],
            on_ground: false,
            object_data: 0,
            data: FxHashMap::default(),
            equipment: Default::default(),
            passengers: Vec::new(),
            vehicle: None,
            leash_holder: None,
            effects: FxHashMap::default(),
            attributes: FxHashMap::default(),
            sim: None,
            fuse: -1,
            swell: (0, 0),
            flight: None,
            lerp: None,
            sim_ticks: 0,
            sim_collided: false,
            interpolation_left: 0,
            steps: None,
        }
    }

    /// `fuseRemainingInTicks` of the TNT renderers: primed TNT reads its synched fuse, a TNT minecart
    /// its client fuse.
    pub fn fuse_remaining(&self, partial: f32) -> Option<f32> {
        let fuse = if self.is_primed_tnt() {
            self.tnt_fuse()
        } else if self.fuse > -1 {
            self.fuse
        } else {
            return None;
        };
        Some(fuse as f32 - partial + 1.0)
    }

    pub fn is_primed_tnt(&self) -> bool {
        self.sim.is_some_and(|s| s.kind == BodyKind::Tnt)
    }

    fn tnt_fuse(&self) -> i32 {
        match self.data.get(&TNT_FUSE) {
            Some(EntityDataValue::Int(f)) => *f,
            _ => TNT_DEFAULT_FUSE,
        }
    }

    pub fn tick_swell(&mut self) {
        const MAX_SWELL: i32 = 30;
        let ignited = matches!(
            self.data.get(&CREEPER_IGNITED),
            Some(EntityDataValue::Boolean(true))
        );
        let dir = match self.data.get(&CREEPER_SWELL_DIR) {
            Some(EntityDataValue::Int(d)) => *d,
            _ => -1,
        };
        let dir = if ignited { 1 } else { dir };
        self.swell.1 = self.swell.0;
        self.swell.0 = self
            .swell
            .0
            .saturating_add(dir.clamp(-1, 1))
            .clamp(0, MAX_SWELL);
    }

    /// `Creeper.getSwelling`.
    pub fn swelling(&self, partial: f32) -> f32 {
        let (now, old) = (self.swell.0 as f32, self.swell.1 as f32);
        (old + (now - old) * partial) / 28.0
    }

    /// The fuse countdown both TNT entities run on the client between server updates.
    fn tick_fuse(&mut self) {
        if self.is_primed_tnt() {
            let fuse = self.tnt_fuse().saturating_sub(1);
            self.data.insert(TNT_FUSE, EntityDataValue::Int(fuse));
        } else if self.fuse > 0 {
            self.fuse -= 1;
        }
    }

    /// Ticks this entity has been simulated locally; drives the item spin and bob.
    pub fn age_ticks(&self) -> u32 {
        self.sim_ticks
    }

    pub fn equipment_slot(&self, slot: EquipmentSlot) -> &ItemStack {
        &self.equipment[slot as usize % self.equipment.len()]
    }

    pub fn set_equipment(&mut self, slot: EquipmentSlot, item: ItemStack) {
        let index = slot as usize % self.equipment.len();
        self.equipment[index] = item;
    }

    /// `lerpHeadTo`: the head turns over the next three ticks instead of snapping.
    pub fn lerp_head_to(&mut self, yaw: f32) {
        self.head_target = yaw;
        self.head_steps = HEAD_STEPS;
    }

    pub fn snap_head(&mut self, yaw: f32) {
        self.head_yaw = yaw;
        self.head_target = yaw;
        self.head_steps = 0;
    }

    fn tick_head(&mut self) {
        if self.head_steps == 0 {
            return;
        }
        self.head_yaw = lerp_degrees(
            self.head_yaw,
            self.head_target,
            1.0 / f32::from(self.head_steps),
        );
        self.head_steps -= 1;
    }

    pub fn is_interpolating(&self) -> bool {
        self.interpolation_left > 0 || self.steps.as_ref().is_some_and(|q| q.len > 0)
    }

    fn mode(&self) -> Lerp {
        if self.sim.is_some() {
            Lerp::Snap
        } else {
            self.lerp.unwrap_or(Lerp::Linear(3))
        }
    }

    /// A position update along a path of `(position, ticks)` steps ending at `end`; an empty path
    /// is a single linear move.
    pub fn move_or_interpolate_to(&mut self, pos: [f64; 3], rot: Option<(f32, f32)>) {
        self.interpolate_path(pos, std::iter::empty(), rot);
    }

    /// Vanilla `moveOrInterpolateTo(PositionPath, yRot, xRot)`.
    pub fn interpolate_path<I>(&mut self, end: [f64; 3], path: I, rot: Option<(f32, f32)>)
    where
        I: Iterator<Item = ([f64; 3], i32)> + Clone,
    {
        let dist_sqr: f64 = (0..3).map(|i| (end[i] - self.pos[i]).powi(2)).sum();
        if dist_sqr > TOO_BIG_TO_INTERPOLATE_SQR {
            let (yaw, pitch) = rot.unwrap_or((self.yaw, self.pitch));
            self.snap_to(end, yaw, pitch);
            return;
        }
        let active = self.is_interpolating();
        let (yaw, pitch) = rot.unwrap_or(if active {
            (self.target_yaw, self.target_pitch)
        } else {
            (self.yaw, self.pitch)
        });
        match self.mode() {
            Lerp::Snap | Lerp::Linear(0) | Lerp::Stepped(0) => {
                self.target_pos = end;
                self.pos = end;
                if rot.is_some() {
                    self.target_yaw = yaw;
                    self.target_pitch = pitch;
                    self.yaw = yaw;
                    self.pitch = pitch;
                }
                self.cancel_interpolation();
            }
            Lerp::Linear(steps) => {
                let same =
                    end == self.target_pos && yaw == self.target_yaw && pitch == self.target_pitch;
                self.set_target(end, yaw, pitch);
                if !active || !same {
                    self.interpolation_left = steps;
                }
            }
            Lerp::Stepped(interval) => {
                let same =
                    end == self.target_pos && yaw == self.target_yaw && pitch == self.target_pitch;
                if active && same {
                    return;
                }
                self.queue_steps(interval, end, path, yaw, pitch);
            }
        }
    }

    /// A rotation-only update never moves the entity, as vanilla's `moveOrInterpolateTo(yRot, xRot)`.
    pub fn rotate_or_interpolate_to(&mut self, yaw: f32, pitch: f32) {
        let at = if self.is_interpolating() {
            self.target_pos
        } else {
            self.pos
        };
        if self.mode() == Lerp::Snap {
            self.target_yaw = yaw;
            self.target_pitch = pitch;
            self.yaw = yaw;
            self.pitch = pitch;
            return;
        }
        let base = self.target_pos;
        self.interpolate_path(at, std::iter::empty(), Some((yaw, pitch)));
        self.target_pos = base;
    }

    fn set_target(&mut self, pos: [f64; 3], yaw: f32, pitch: f32) {
        self.target_pos = pos;
        self.target_yaw = yaw;
        self.target_pitch = pitch;
    }

    /// `SteppedInterpolationHandler#startInterpolating`.
    fn queue_steps<I>(&mut self, interval: u8, end: [f64; 3], path: I, yaw: f32, pitch: f32)
    where
        I: Iterator<Item = ([f64; 3], i32)> + Clone,
    {
        let active = self.is_interpolating();
        let (old_pos, old_yaw, old_pitch) = (self.target_pos, self.target_yaw, self.target_pitch);
        let start = Step {
            pos: self.pos,
            yaw: self.yaw,
            pitch: self.pitch,
            ticks: 0,
        };
        let q = self.steps.get_or_insert_with(Default::default);
        if !active {
            q.last = start;
            q.current_ticks = 1.0;
        }
        let interval = i32::from(interval);
        let mut stepped = path.clone().peekable();
        if end == old_pos || stepped.peek().is_none() {
            q.push(end, yaw, pitch, interval);
        } else if yaw == old_yaw && pitch == old_pitch {
            for (pos, ticks) in path {
                q.push(pos, yaw, pitch, ticks);
            }
        } else {
            let total: i64 = path
                .clone()
                .map(|(_, t)| i64::from(t.clamp(0, MAX_STEP_TICKS)))
                .sum();
            let mut offset = 0i64;
            for (pos, ticks) in path {
                offset += i64::from(ticks.clamp(0, MAX_STEP_TICKS));
                let a = if total > 0 {
                    offset as f32 / total as f32
                } else {
                    1.0
                };
                q.push(
                    pos,
                    lerp_degrees(old_yaw, yaw, a),
                    old_pitch + (pitch - old_pitch) * a,
                    ticks,
                );
            }
        }
        self.set_target(end, yaw, pitch);
    }

    pub fn snap_to(&mut self, pos: [f64; 3], yaw: f32, pitch: f32) {
        self.set_target(pos, yaw, pitch);
        self.pos = pos;
        self.yaw = yaw;
        self.pitch = pitch;
        self.cancel_interpolation();
    }

    fn cancel_interpolation(&mut self) {
        self.interpolation_left = 0;
        if let Some(q) = &mut self.steps {
            q.reset();
        }
    }

    pub fn tick_interpolation(&mut self) {
        self.tick_interpolation_at(1.0);
    }

    fn tick_interpolation_at(&mut self, tick_speed: f32) {
        if let Lerp::Stepped(interval) = self.mode()
            && let Some(q) = self.steps.as_mut().filter(|q| q.len > 0)
        {
            let (pos, yaw, pitch) =
                q.sample()
                    .unwrap_or((self.target_pos, self.target_yaw, self.target_pitch));
            q.advance(tick_speed, interval);
            self.pos = pos;
            self.yaw = yaw;
            self.pitch = pitch;
            return;
        }
        if self.interpolation_left == 0 {
            return;
        }
        let t = 1.0 / f64::from(self.interpolation_left);
        for i in 0..3 {
            self.pos[i] += (self.target_pos[i] - self.pos[i]) * t;
        }
        self.yaw = lerp_degrees(self.yaw, self.target_yaw, t as f32);
        self.pitch += (self.target_pitch - self.pitch) * t as f32;
        self.interpolation_left -= 1;
    }

    fn flag(&self, index: u8) -> bool {
        matches!(self.data.get(&index), Some(EntityDataValue::Boolean(true)))
    }

    fn byte(&self, index: u8) -> i8 {
        match self.data.get(&index) {
            Some(EntityDataValue::Byte(b)) => *b,
            _ => 0,
        }
    }

    /// The entity a rocket rides with, or the owner a loyal trident with no physics returns to.
    fn followed(&self, kind: BodyKind) -> Option<i32> {
        match kind {
            BodyKind::FireworkRocket => match self.data.get(&FIREWORK_ATTACHED) {
                Some(EntityDataValue::OptionalUnsignedInt(Some(id))) => i32::try_from(*id).ok(),
                _ => None,
            },
            BodyKind::Trident
                if self.byte(TRIDENT_LOYALTY) > 0
                    && self.byte(ARROW_FLAGS) & ARROW_NO_PHYSICS != 0 =>
            {
                Some(self.object_data)
            }
            _ => None,
        }
    }

    fn body_flags(&self, kind: BodyKind, anchor: Option<[f64; 3]>) -> BodyFlags {
        let arrow = matches!(kind, BodyKind::Arrow | BodyKind::Trident);
        BodyFlags {
            no_gravity: self.flag(NO_GRAVITY),
            no_physics: arrow && self.byte(ARROW_FLAGS) & ARROW_NO_PHYSICS != 0,
            dangerous: kind == BodyKind::WitherSkull && self.flag(WITHER_SKULL_DANGEROUS),
            shot_at_angle: kind == BodyKind::FireworkRocket && self.flag(FIREWORK_SHOT_AT_ANGLE),
            loyalty: if kind == BodyKind::Trident {
                self.byte(TRIDENT_LOYALTY).clamp(0, 127) as u8
            } else {
                0
            },
            anchor,
        }
    }

    fn tick_sim<W: CollisionWorld + ?Sized>(
        &mut self,
        shape: SimShape,
        cx: &mut SimContext<'_, W>,
        anchor: Option<[f64; 3]>,
    ) {
        let mut body = Body::new(
            shape.kind,
            self.id,
            (shape.width, shape.height),
            self.pos,
            self.velocity,
        );
        body.tick_count = self.sim_ticks;
        body.on_ground = self.on_ground;
        body.horizontal_collision = self.sim_collided;
        body.flags = self.body_flags(shape.kind, anchor);
        body.tick(cx.world, cx.data, cx.scratch, cx.follow);
        self.sim_ticks = body.tick_count;
        self.pos = body.pos;
        self.velocity = body.vel;
        self.on_ground = body.on_ground;
        self.sim_collided = body.horizontal_collision;
        if shape.kind.is_projectile() && body.vel != [0.0; 3] {
            let (yaw, pitch) = mcr_physics::aim(body.vel);
            self.yaw = lerp_degrees(self.yaw, yaw, 0.2);
            self.pitch = lerp_degrees(self.pitch, pitch, 0.2);
            self.target_yaw = self.yaw;
            self.target_pitch = self.pitch;
        }
    }

    pub fn set_passengers(&mut self, passengers: &[i32]) {
        self.passengers.clear();
        self.passengers
            .extend(passengers.iter().copied().take(MAX_PASSENGERS));
    }

    pub fn upsert_effect(&mut self, id: i32, effect: MobEffect) {
        if self.effects.contains_key(&id) || self.effects.len() < MAX_EFFECTS {
            self.effects.insert(id, effect);
        }
    }

    pub fn upsert_attribute(&mut self, id: i32, attribute: AttributeState) {
        if self.attributes.contains_key(&id) || self.attributes.len() < MAX_ATTRIBUTES {
            self.attributes.insert(id, attribute);
        }
    }
}

const HEAD_STEPS: u8 = 3;

fn lerp_degrees(from: f32, to: f32, t: f32) -> f32 {
    let diff = (to - from + 180.0).rem_euclid(360.0) - 180.0;
    from + diff * t
}

pub fn encode_delta(v: f64) -> i64 {
    (v * DELTA_SCALE).round() as i64
}

fn decode_delta(v: i64) -> f64 {
    v as f64 / DELTA_SCALE
}

/// Vanilla `VecDeltaCodec.decode`: axes with a zero delta keep the base value exactly.
pub fn apply_delta(base: [f64; 3], xa: i16, ya: i16, za: i16) -> [f64; 3] {
    let axis = |b: f64, d: i16| {
        if d == 0 {
            b
        } else {
            decode_delta(encode_delta(b) + i64::from(d))
        }
    };
    [axis(base[0], xa), axis(base[1], ya), axis(base[2], za)]
}

pub fn end_of_delta(base: [f64; 3], delta: &VecDelta) -> [f64; 3] {
    match delta {
        VecDelta::Linear { xa, ya, za } => apply_delta(base, *xa, *ya, *za),
        VecDelta::Stepped(steps) => steps
            .iter()
            .fold(base, |pos, s| apply_delta(pos, s.xa, s.ya, s.za)),
    }
}

pub fn end_of_path(path: &PositionPath) -> [f64; 3] {
    match path {
        PositionPath::Linear(v) => [v.x, v.y, v.z],
        PositionPath::Stepped(steps) => steps
            .last()
            .map_or([0.0; 3], |s| [s.position.x, s.position.y, s.position.z]),
    }
}

/// Entities by network id; the local player is not among them.
#[derive(Debug, Clone, Default)]
pub struct Entities {
    map: FxHashMap<i32, Entity>,
    anchors: Vec<(i32, [f64; 3])>,
}

impl Entities {
    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn get(&self, id: i32) -> Option<&Entity> {
        self.map.get(&id)
    }

    pub fn get_mut(&mut self, id: i32) -> Option<&mut Entity> {
        self.map.get_mut(&id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Entity> {
        self.map.values()
    }

    pub fn by_uuid(&self, uuid: Uuid) -> Option<&Entity> {
        self.map.values().find(|e| e.uuid == uuid)
    }

    pub fn insert(&mut self, entity: Entity) -> bool {
        if !self.map.contains_key(&entity.id) && self.map.len() >= MAX_ENTITIES {
            return false;
        }
        self.map.insert(entity.id, entity);
        true
    }

    pub fn remove(&mut self, id: i32) -> Option<Entity> {
        let removed = self.map.remove(&id)?;
        if let Some(vehicle) = removed.vehicle.and_then(|v| self.map.get_mut(&v)) {
            vehicle.passengers.retain(|&p| p != id);
        }
        for passenger in &removed.passengers {
            if let Some(e) = self.map.get_mut(passenger) {
                e.vehicle = None;
            }
        }
        Some(removed)
    }

    pub fn clear(&mut self) {
        self.map.clear();
    }

    /// Where each rocket's rider and each returning trident's owner is, before anything moves.
    fn collect_anchors<W: CollisionWorld + ?Sized>(&mut self, cx: &SimContext<'_, W>) {
        self.anchors.clear();
        for e in self.map.values() {
            let Some(followed) = e.sim.and_then(|s| e.followed(s.kind)) else {
                continue;
            };
            let rocket = e.sim.is_some_and(|s| s.kind == BodyKind::FireworkRocket);
            let at = match cx.local {
                Some((id, feet, eye)) if id == followed => {
                    Some([feet[0], feet[1] + if rocket { 0.0 } else { eye }, feet[2]])
                }
                _ => self.map.get(&followed).map(|o| {
                    let eye = if rocket {
                        0.0
                    } else {
                        u32::try_from(o.kind)
                            .ok()
                            .and_then(|k| cx.version.entity_type_info(k))
                            .map_or(0.0, |i| f64::from(i.eye_height))
                    };
                    [o.pos[0], o.pos[1] + eye, o.pos[2]]
                }),
            };
            if let Some(at) = at
                && self.anchors.len() < MAX_SIMULATED_PER_TICK
            {
                self.anchors.push((e.id, at));
            }
        }
        self.anchors.sort_unstable_by_key(|&(id, _)| id);
    }

    pub(crate) fn tick<W: CollisionWorld + ?Sized>(&mut self, cx: &mut SimContext<'_, W>) {
        self.collect_anchors(cx);
        let mut simulated = 0usize;
        let anchors = &self.anchors;
        for entity in self.map.values_mut() {
            if cx
                .frozen_except
                .is_some_and(|player| entity.kind != player && entity.passengers.is_empty())
            {
                continue;
            }
            entity.tick_fuse();
            entity.tick_head();
            if cx.creeper_type == Some(entity.kind) {
                entity.tick_swell();
            }
            match entity.sim {
                Some(_) if simulated >= MAX_SIMULATED_PER_TICK => {}
                Some(shape) => {
                    simulated += 1;
                    let anchor = anchors
                        .binary_search_by_key(&entity.id, |&(id, _)| id)
                        .ok()
                        .map(|i| anchors[i].1);
                    entity.tick_sim(shape, cx, anchor);
                }
                None => {
                    entity.tick_interpolation_at(cx.tick_speed);
                    if cx.dragon_type == Some(entity.kind) {
                        let (y, yaw) = (entity.pos[1], entity.yaw);
                        entity
                            .flight
                            .get_or_insert_with(Default::default)
                            .record(y, yaw);
                    }
                }
            }
        }
    }

    /// Replaces a vehicle's passenger list and fixes up both sides of the link.
    pub fn set_passengers(&mut self, vehicle: i32, passengers: &[i32]) -> bool {
        let Some(old) = self.map.get(&vehicle).map(|v| v.passengers.clone()) else {
            return false;
        };
        for id in old {
            if let Some(e) = self.map.get_mut(&id)
                && e.vehicle == Some(vehicle)
            {
                e.vehicle = None;
            }
        }
        for &id in passengers.iter().take(MAX_PASSENGERS) {
            if let Some(e) = self.map.get_mut(&id) {
                e.vehicle = Some(vehicle);
            }
        }
        if let Some(v) = self.map.get_mut(&vehicle) {
            v.set_passengers(passengers);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delta_matches_vanilla_fixed_point() {
        let base = [10.0, 64.0, -3.5];
        let moved = apply_delta(base, 4096, 0, -2048);
        assert_eq!(moved, [11.0, 64.0, -4.0]);
        assert_eq!(apply_delta(base, 0, 0, 0), base);
    }

    #[test]
    fn delta_snaps_unaligned_base_to_grid() {
        let moved = apply_delta([0.00001, 0.0, 0.0], 1, 0, 0);
        assert_eq!(moved[0], 1.0 / 4096.0);
    }

    #[test]
    fn rotation_updates_keep_a_simulated_entity_in_place() {
        let mut e = Entity::new(1, Uuid::nil(), 0, [0.0; 3]);
        e.sim = Some(SimShape {
            kind: BodyKind::Item,
            width: 0.25,
            height: 0.25,
        });
        e.pos = [2.0, 1.0, 0.0];
        e.rotate_or_interpolate_to(90.0, 10.0);
        assert_eq!(e.pos, [2.0, 1.0, 0.0]);
        assert_eq!((e.yaw, e.pitch), (90.0, 10.0));
    }

    #[test]
    fn dragon_leans_into_climbs() {
        let mut f = FlightHistory::default();
        for t in 0..20 {
            f.record(f64::from(t) * 0.1, 30.0);
        }
        let (yaw, lean) = f.lean(0.0);
        assert!((yaw - 30.0).abs() < 1e-3);
        assert!((lean - 5.0).abs() < 1e-2, "{lean}");
        let flat = {
            let mut g = FlightHistory::default();
            for _ in 0..20 {
                g.record(70.0, 0.0);
            }
            g.lean(0.5).1
        };
        assert_eq!(flat, 0.0);
    }

    #[test]
    fn head_turns_over_three_ticks() {
        let mut e = Entity::new(1, Uuid::nil(), 0, [0.0; 3]);
        e.lerp_head_to(90.0);
        e.tick_head();
        assert!((e.head_yaw - 30.0).abs() < 1e-4);
        e.tick_head();
        e.tick_head();
        assert!((e.head_yaw - 90.0).abs() < 1e-4);
        e.tick_head();
        assert!((e.head_yaw - 90.0).abs() < 1e-4);
    }

    #[test]
    fn interpolation_reaches_target_in_three_ticks() {
        let mut e = Entity::new(1, Uuid::nil(), 0, [0.0; 3]);
        e.move_or_interpolate_to([3.0, 0.0, 0.0], Some((90.0, 0.0)));
        for _ in 0..3 {
            e.tick_interpolation();
        }
        assert_eq!(e.pos, [3.0, 0.0, 0.0]);
        assert!((e.yaw - 90.0).abs() < 1e-4);
        assert!(!e.is_interpolating());
    }

    #[test]
    fn huge_jumps_snap() {
        let mut e = Entity::new(1, Uuid::nil(), 0, [0.0; 3]);
        e.move_or_interpolate_to([1000.0, 0.0, 0.0], None);
        assert_eq!(e.pos, [1000.0, 0.0, 0.0]);
    }

    #[test]
    fn passengers_link_both_ways() {
        let mut all = Entities::default();
        for id in 1..=3 {
            all.insert(Entity::new(id, Uuid::nil(), 0, [0.0; 3]));
        }
        assert!(all.set_passengers(1, &[2, 3]));
        assert_eq!(all.get(2).unwrap().vehicle, Some(1));
        assert!(all.set_passengers(1, &[3]));
        assert_eq!(all.get(2).unwrap().vehicle, None);
        all.remove(1);
        assert_eq!(all.get(3).unwrap().vehicle, None);
    }

    #[test]
    fn tnt_fuses_count_down_on_the_client() {
        let mut tnt = Entity::new(1, Uuid::nil(), 0, [0.0; 3]);
        tnt.sim = Some(SimShape {
            kind: BodyKind::Tnt,
            width: 0.98,
            height: 0.98,
        });
        assert_eq!(tnt.fuse_remaining(0.0), Some(81.0));
        tnt.data.insert(TNT_FUSE, EntityDataValue::Int(9));
        tnt.tick_fuse();
        assert_eq!(tnt.fuse_remaining(0.5), Some(8.5));
        let mut cart = Entity::new(2, Uuid::nil(), 0, [0.0; 3]);
        cart.tick_fuse();
        assert_eq!(cart.fuse_remaining(0.0), None);
        cart.fuse = 1;
        cart.tick_fuse();
        cart.tick_fuse();
        assert_eq!(cart.fuse, 0, "stays at zero until the server removes it");
    }

    #[test]
    fn entity_cap_holds() {
        let mut all = Entities::default();
        for id in 0..MAX_ENTITIES as i32 {
            assert!(all.insert(Entity::new(id, Uuid::nil(), 0, [0.0; 3])));
        }
        assert!(!all.insert(Entity::new(-1, Uuid::nil(), 0, [0.0; 3])));
    }
}
