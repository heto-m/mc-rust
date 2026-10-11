use mcr_data::Version;
use mcr_physics::{Body, BodyKind, BodyScratch, CollisionWorld, PhysicsData};
use mcr_protocol::play::clientbound::entity::{PositionPath, VecDelta};
use mcr_protocol::types::ItemStack;
use mcr_protocol::types::component::{AttributeModifier, EquipmentSlot};
use mcr_protocol::types::entity_data::EntityDataValue;
use rustc_hash::FxHashMap;
use uuid::Uuid;

const TNT_FUSE: u8 = 8;
const TNT_DEFAULT_FUSE: i32 = 80;
pub const MINECART_TNT_FUSE: i32 = 80;

pub const MAX_ENTITIES: usize = 1 << 16;
const MAX_SIMULATED_PER_TICK: usize = 4096;
const MAX_PASSENGERS: usize = 4096;
const MAX_EFFECTS: usize = 64;
const MAX_ATTRIBUTES: usize = 128;
const INTERPOLATION_STEPS: u8 = 3;
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
}

impl SimKinds {
    pub fn resolve(data: &Version) -> Self {
        const NAMES: [(&str, BodyKind); 12] = [
            ("item", BodyKind::Item),
            ("experience_orb", BodyKind::ExperienceOrb),
            ("tnt", BodyKind::Tnt),
            ("falling_block", BodyKind::FallingBlock),
            ("snowball", BodyKind::Thrown),
            ("egg", BodyKind::Thrown),
            ("ender_pearl", BodyKind::Thrown),
            ("experience_bottle", BodyKind::ExperienceBottle),
            ("splash_potion", BodyKind::Potion),
            ("lingering_potion", BodyKind::Potion),
            ("arrow", BodyKind::Arrow),
            ("spectral_arrow", BodyKind::Arrow),
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
        Self {
            by_type,
            player_type,
        }
    }

    pub fn get(&self, entity_type: i32) -> Option<SimShape> {
        self.by_type.get(&entity_type).copied()
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
}

impl AttributeState {
    /// Vanilla order: flat additions, then summed base multipliers, then each total multiplier.
    pub fn value(&self, skip_modifier: Option<&str>) -> f64 {
        use mcr_protocol::types::component::AttributeOperation as Op;
        let mut value = self.base;
        let mods = self
            .modifiers
            .iter()
            .filter(|m| Some(m.id.as_str()) != skip_modifier);
        let (mut mul_base, mut mul_total) = (0.0, Vec::new());
        for m in mods {
            match m.operation {
                Op::AddValue => value += m.amount,
                Op::AddMultipliedBase => mul_base += m.amount,
                Op::AddMultipliedTotal => mul_total.push(m.amount),
            }
        }
        let base_for_mul = value;
        value += base_for_mul * mul_base;
        for m in mul_total {
            value *= 1.0 + m;
        }
        value
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
    sim_ticks: u32,
    interpolation_left: u8,
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
            sim_ticks: 0,
            interpolation_left: 0,
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

    pub fn is_interpolating(&self) -> bool {
        self.interpolation_left > 0
    }

    pub fn move_or_interpolate_to(&mut self, pos: [f64; 3], rot: Option<(f32, f32)>) {
        let dist_sqr: f64 = (0..3).map(|i| (pos[i] - self.pos[i]).powi(2)).sum();
        self.target_pos = pos;
        if let Some((yaw, pitch)) = rot {
            self.target_yaw = yaw;
            self.target_pitch = pitch;
        }
        if self.sim.is_some() || dist_sqr > TOO_BIG_TO_INTERPOLATE_SQR {
            self.snap();
        } else {
            self.interpolation_left = INTERPOLATION_STEPS;
        }
    }

    /// A rotation-only update never moves the entity, as vanilla's `moveOrInterpolateTo(yRot, xRot)`.
    pub fn rotate_or_interpolate_to(&mut self, yaw: f32, pitch: f32) {
        self.target_yaw = yaw;
        self.target_pitch = pitch;
        if self.sim.is_some() {
            self.yaw = yaw;
            self.pitch = pitch;
        } else {
            self.interpolation_left = INTERPOLATION_STEPS;
        }
    }

    pub fn snap_to(&mut self, pos: [f64; 3], yaw: f32, pitch: f32) {
        self.target_pos = pos;
        self.target_yaw = yaw;
        self.target_pitch = pitch;
        self.snap();
    }

    fn snap(&mut self) {
        self.pos = self.target_pos;
        self.yaw = self.target_yaw;
        self.pitch = self.target_pitch;
        self.interpolation_left = 0;
    }

    pub fn tick_interpolation(&mut self) {
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

    fn tick_sim<W: CollisionWorld + ?Sized>(
        &mut self,
        shape: SimShape,
        cx: &mut SimContext<'_, W>,
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
        body.tick(cx.world, cx.data, cx.scratch, cx.follow);
        self.sim_ticks = body.tick_count;
        self.pos = body.pos;
        self.velocity = body.vel;
        self.on_ground = body.on_ground;
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

    pub(crate) fn tick<W: CollisionWorld + ?Sized>(&mut self, cx: &mut SimContext<'_, W>) {
        let mut simulated = 0usize;
        for entity in self.map.values_mut() {
            if cx
                .frozen_except
                .is_some_and(|player| entity.kind != player && entity.passengers.is_empty())
            {
                continue;
            }
            entity.tick_fuse();
            match entity.sim {
                Some(_) if simulated >= MAX_SIMULATED_PER_TICK => {}
                Some(shape) => {
                    simulated += 1;
                    entity.tick_sim(shape, cx);
                }
                None => entity.tick_interpolation(),
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
