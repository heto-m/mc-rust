use mcr_buf::{
    Angle, Decode, DecodeError, Encode, Identifier, LengthPrefixed, Result, read_varint,
    write_varint,
};
use uuid::Uuid;

use crate::packet;
use crate::types::{
    EntityDataValues, ItemStack, Vec3d,
    component::{self, AttributeModifier, EquipmentSlot},
    game::Hand,
};

pub mod varint_list {
    use mcr_buf::{Result, prealloc_capacity, read_len, read_varint, write_varint};

    pub fn encode(values: &[i32], out: &mut Vec<u8>) {
        write_varint(out, values.len() as i32);
        for &v in values {
            write_varint(out, v);
        }
    }

    pub fn decode(input: &mut &[u8]) -> Result<Vec<i32>> {
        let len = read_len(input, input.len())?;
        let mut values = Vec::with_capacity(prealloc_capacity::<i32>(len));
        for _ in 0..len {
            values.push(read_varint(input)?);
        }
        Ok(values)
    }
}

/// Entity id written as `id + 1` so that `-1` (none) costs one byte.
pub mod optional_entity_id {
    use mcr_buf::{Result, read_varint, write_varint};

    pub fn encode(id: &i32, out: &mut Vec<u8>) {
        write_varint(out, id.wrapping_add(1));
    }

    pub fn decode(input: &mut &[u8]) -> Result<i32> {
        Ok(read_varint(input)?.wrapping_sub(1))
    }
}

/// Velocity as vanilla's low-precision vector: lossy, so only values that survive
/// quantisation roundtrip exactly.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LpVec3(pub Vec3d);

impl LpVec3 {
    const MAX_QUANTIZED: f64 = 32766.0;
    pub const ABS_MAX: f64 = 1.7179869183E10;
    pub const ABS_MIN: f64 = 3.051944088384301E-5;

    fn sanitize(v: f64) -> f64 {
        if v.is_nan() {
            0.0
        } else {
            v.clamp(-Self::ABS_MAX, Self::ABS_MAX)
        }
    }

    fn pack(v: f64) -> u64 {
        ((v * 0.5 + 0.5) * Self::MAX_QUANTIZED + 0.5).floor() as u64
    }

    fn unpack(v: u64) -> f64 {
        ((v & 0x7fff) as f64).min(Self::MAX_QUANTIZED) * 2.0 / Self::MAX_QUANTIZED - 1.0
    }
}

impl Encode for LpVec3 {
    fn encode(&self, out: &mut Vec<u8>) {
        let (x, y, z) = (
            Self::sanitize(self.0.x),
            Self::sanitize(self.0.y),
            Self::sanitize(self.0.z),
        );
        let largest = x.abs().max(y.abs()).max(z.abs());
        if largest < Self::ABS_MIN {
            out.push(0);
            return;
        }
        let scale = largest.ceil() as u64;
        let partial = scale & 3 != scale;
        let markers = if partial { scale & 3 | 4 } else { scale };
        let scale = scale as f64;
        let buffer = markers
            | Self::pack(x / scale) << 3
            | Self::pack(y / scale) << 18
            | Self::pack(z / scale) << 33;
        out.push(buffer as u8);
        out.push((buffer >> 8) as u8);
        ((buffer >> 16) as u32).encode(out);
        if partial {
            write_varint(out, (scale as u64 >> 2) as i32);
        }
    }
}

impl Decode<'_> for LpVec3 {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        let lowest = u8::decode(input)?;
        if lowest == 0 {
            return Ok(Self::default());
        }
        let middle = u8::decode(input)?;
        let highest = u32::decode(input)?;
        let buffer = u64::from(highest) << 16 | u64::from(middle) << 8 | u64::from(lowest);
        let mut scale = u64::from(lowest & 3);
        if lowest & 4 != 0 {
            scale |= u64::from(read_varint(input)? as u32) << 2;
        }
        let scale = scale as f64;
        Ok(Self(Vec3d {
            x: Self::unpack(buffer >> 3) * scale,
            y: Self::unpack(buffer >> 18) * scale,
            z: Self::unpack(buffer >> 33) * scale,
        }))
    }
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct DeltaStep {
    #[mcr(varint)]
    pub ticks: i32,
    pub xa: i16,
    pub ya: i16,
    pub za: i16,
}

/// Relative movement in 1/4096 block units; the step count travels in the packet's property
/// VarInt, not here.
#[derive(Debug, Clone, PartialEq)]
pub enum VecDelta {
    Linear { xa: i16, ya: i16, za: i16 },
    Stepped(Vec<DeltaStep>),
}

impl VecDelta {
    const MIN_BYTES_PER_STEP: usize = 7;

    pub fn step_count(&self) -> usize {
        match self {
            Self::Linear { .. } => 0,
            Self::Stepped(steps) => steps.len(),
        }
    }

    fn encode_body(&self, out: &mut Vec<u8>) {
        match self {
            Self::Linear { xa, ya, za } => {
                xa.encode(out);
                ya.encode(out);
                za.encode(out);
            }
            Self::Stepped(steps) => {
                for step in steps {
                    step.encode(out);
                }
            }
        }
    }

    fn decode_body(input: &mut &[u8], step_count: usize) -> Result<Self> {
        if step_count == 0 {
            return Ok(Self::Linear {
                xa: i16::decode(input)?,
                ya: i16::decode(input)?,
                za: i16::decode(input)?,
            });
        }
        let max = input.len() / Self::MIN_BYTES_PER_STEP;
        if step_count > max {
            return Err(DecodeError::TooLong {
                len: step_count,
                max,
            });
        }
        let mut steps = Vec::with_capacity(step_count);
        for _ in 0..step_count {
            steps.push(DeltaStep::decode(input)?);
        }
        Ok(Self::Stepped(steps))
    }
}

fn encode_movement(entity_id: i32, on_ground: bool, delta: &VecDelta, out: &mut Vec<u8>) {
    write_varint(out, entity_id);
    write_varint(out, (delta.step_count() << 1) as i32 | i32::from(on_ground));
    delta.encode_body(out);
}

fn decode_movement(input: &mut &[u8]) -> Result<(i32, bool, VecDelta)> {
    let entity_id = read_varint(input)?;
    let properties = read_varint(input)? as u32;
    let delta = VecDelta::decode_body(input, (properties >> 1) as usize)?;
    Ok((entity_id, properties & 1 != 0, delta))
}

#[derive(Debug, Clone, PartialEq)]
pub struct MoveEntityPos {
    pub entity_id: i32,
    pub delta: VecDelta,
    pub on_ground: bool,
}

impl Encode for MoveEntityPos {
    fn encode(&self, out: &mut Vec<u8>) {
        encode_movement(self.entity_id, self.on_ground, &self.delta, out);
    }
}

impl Decode<'_> for MoveEntityPos {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        let (entity_id, on_ground, delta) = decode_movement(input)?;
        Ok(Self {
            entity_id,
            delta,
            on_ground,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MoveEntityPosRot {
    pub entity_id: i32,
    pub delta: VecDelta,
    pub y_rot: Angle,
    pub x_rot: Angle,
    pub on_ground: bool,
}

impl Encode for MoveEntityPosRot {
    fn encode(&self, out: &mut Vec<u8>) {
        encode_movement(self.entity_id, self.on_ground, &self.delta, out);
        self.y_rot.encode(out);
        self.x_rot.encode(out);
    }
}

impl Decode<'_> for MoveEntityPosRot {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        let (entity_id, on_ground, delta) = decode_movement(input)?;
        Ok(Self {
            entity_id,
            delta,
            y_rot: Angle::decode(input)?,
            x_rot: Angle::decode(input)?,
            on_ground,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct MoveEntityRot {
    #[mcr(varint)]
    pub entity_id: i32,
    pub on_ground: bool,
    pub y_rot: Angle,
    pub x_rot: Angle,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct MinecartStep {
    pub position: Vec3d,
    pub movement: Vec3d,
    pub y_rot: Angle,
    pub x_rot: Angle,
    pub weight: f32,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct MoveMinecartAlongTrack {
    #[mcr(varint)]
    pub entity_id: i32,
    pub steps: Vec<MinecartStep>,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct MoveVehicle {
    pub position: Vec3d,
    pub y_rot: f32,
    pub x_rot: f32,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct PositionStep {
    pub position: Vec3d,
    #[mcr(varint)]
    pub tick_offset: i32,
}

/// Stepped paths end at their last step, so no end position is sent.
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub enum PositionPath {
    Linear(Vec3d),
    Stepped(Vec<PositionStep>),
}

impl PositionPath {
    pub fn end_position(&self) -> Option<Vec3d> {
        match self {
            Self::Linear(end) => Some(*end),
            Self::Stepped(steps) => steps.last().map(|s| s.position),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct EntityPositionSync {
    #[mcr(varint)]
    pub entity_id: i32,
    pub position: PositionPath,
    pub y_rot: f32,
    pub x_rot: f32,
    pub on_ground: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Encode, Decode)]
pub struct Relatives(pub u32);

impl Relatives {
    pub const X: u32 = 1;
    pub const Y: u32 = 1 << 1;
    pub const Z: u32 = 1 << 2;
    pub const Y_ROT: u32 = 1 << 3;
    pub const X_ROT: u32 = 1 << 4;
    pub const DELTA_X: u32 = 1 << 5;
    pub const DELTA_Y: u32 = 1 << 6;
    pub const DELTA_Z: u32 = 1 << 7;
    pub const ROTATE_DELTA: u32 = 1 << 8;

    pub const fn contains(self, flag: u32) -> bool {
        self.0 & flag == flag
    }
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct PositionMoveRotation {
    pub position: Vec3d,
    pub delta_movement: Vec3d,
    pub y_rot: f32,
    pub x_rot: f32,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct TeleportEntity {
    #[mcr(varint)]
    pub entity_id: i32,
    pub change: PositionMoveRotation,
    pub relatives: Relatives,
    pub on_ground: bool,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct AddEntity {
    #[mcr(varint)]
    pub entity_id: i32,
    pub uuid: Uuid,
    #[mcr(varint)]
    pub entity_type: i32,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub velocity: LpVec3,
    pub x_rot: Angle,
    pub y_rot: Angle,
    pub y_head_rot: Angle,
    #[mcr(varint)]
    pub data: i32,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct Animate {
    #[mcr(varint)]
    pub entity_id: i32,
    pub action: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct EntitySwing {
    #[mcr(varint)]
    pub entity_id: i32,
    pub hand: Hand,
    pub animation: component::SwingAnimation,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct DamageEvent {
    #[mcr(varint)]
    pub entity_id: i32,
    #[mcr(varint)]
    pub source_type: i32,
    #[mcr(with = optional_entity_id)]
    pub source_cause_id: i32,
    #[mcr(with = optional_entity_id)]
    pub source_direct_id: i32,
    pub source_position: Option<Vec3d>,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct EntityEvent {
    pub entity_id: i32,
    pub event_id: i8,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct HurtAnimation {
    #[mcr(varint)]
    pub entity_id: i32,
    pub yaw: f32,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct ProjectilePower {
    #[mcr(varint)]
    pub entity_id: i32,
    pub acceleration_power: f64,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct RemoveEntities {
    #[mcr(with = varint_list)]
    pub entity_ids: Vec<i32>,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct RemoveMobEffect {
    #[mcr(varint)]
    pub entity_id: i32,
    #[mcr(varint)]
    pub effect: i32,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct RotateHead {
    #[mcr(varint)]
    pub entity_id: i32,
    pub y_head_rot: Angle,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct SetEntityData {
    #[mcr(varint)]
    pub entity_id: i32,
    pub values: EntityDataValues,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct SetEntityLink {
    pub source_id: i32,
    pub dest_id: i32,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct SetEntityMotion {
    #[mcr(varint)]
    pub entity_id: i32,
    pub movement: LpVec3,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EquipmentEntry {
    pub slot: EquipmentSlot,
    pub item: ItemStack,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SetEquipment {
    pub entity_id: i32,
    pub slots: Vec<EquipmentEntry>,
}

impl Encode for SetEquipment {
    fn encode(&self, out: &mut Vec<u8>) {
        write_varint(out, self.entity_id);
        for (i, entry) in self.slots.iter().enumerate() {
            let more = if i + 1 < self.slots.len() { 0x80 } else { 0 };
            out.push(entry.slot.ordinal() | more);
            entry.item.encode(out);
        }
    }
}

impl Decode<'_> for SetEquipment {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        let entity_id = read_varint(input)?;
        let mut slots = Vec::new();
        loop {
            let byte = u8::decode(input)?;
            let slot = *EquipmentSlot::ALL.get(usize::from(byte & 0x7f)).ok_or(
                DecodeError::InvalidValue {
                    what: "equipment slot",
                    value: i64::from(byte & 0x7f),
                },
            )?;
            slots.push(EquipmentEntry {
                slot,
                item: ItemStack::decode(input)?,
            });
            if byte & 0x80 == 0 {
                break;
            }
        }
        Ok(Self { entity_id, slots })
    }
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct SetPassengers {
    #[mcr(varint)]
    pub vehicle: i32,
    #[mcr(with = varint_list)]
    pub passengers: Vec<i32>,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct TakeItemEntity {
    #[mcr(varint)]
    pub item_id: i32,
    #[mcr(varint)]
    pub player_id: i32,
    #[mcr(varint)]
    pub amount: i32,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct AttributeSnapshot {
    #[mcr(varint)]
    pub attribute: i32,
    pub base: f64,
    pub modifiers: Vec<AttributeModifier>,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct UpdateAttributes {
    #[mcr(varint)]
    pub entity_id: i32,
    pub attributes: LengthPrefixed<AttributeSnapshot, 128>,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct UpdateMobEffect {
    #[mcr(varint)]
    pub entity_id: i32,
    #[mcr(varint)]
    pub effect: i32,
    #[mcr(varint)]
    pub amplifier: i32,
    #[mcr(varint)]
    pub duration_ticks: i32,
    pub flags: u8,
}

impl UpdateMobEffect {
    pub const FLAG_AMBIENT: u8 = 1;
    pub const FLAG_VISIBLE: u8 = 2;
    pub const FLAG_SHOW_ICON: u8 = 4;
    pub const FLAG_BLEND: u8 = 8;
}

#[derive(Debug, Clone, PartialEq)]
pub enum WaypointId {
    Uuid(Uuid),
    Name(String),
}

impl Encode for WaypointId {
    fn encode(&self, out: &mut Vec<u8>) {
        match self {
            Self::Uuid(uuid) => {
                true.encode(out);
                uuid.encode(out);
            }
            Self::Name(name) => {
                false.encode(out);
                name.encode(out);
            }
        }
    }
}

impl Decode<'_> for WaypointId {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        if bool::decode(input)? {
            Uuid::decode(input).map(Self::Uuid)
        } else {
            String::decode(input).map(Self::Name)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct WaypointIcon {
    pub style: Identifier,
    pub color: Option<[u8; 3]>,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub enum WaypointData {
    Empty,
    Position {
        #[mcr(varint)]
        x: i32,
        #[mcr(varint)]
        y: i32,
        #[mcr(varint)]
        z: i32,
    },
    Chunk {
        #[mcr(varint)]
        x: i32,
        #[mcr(varint)]
        z: i32,
    },
    Azimuth {
        angle: f32,
    },
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct TrackedWaypoint {
    pub id: WaypointId,
    pub icon: WaypointIcon,
    pub data: WaypointData,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum WaypointOperation {
    Track,
    Untrack,
    Update,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct Waypoint {
    pub operation: WaypointOperation,
    pub waypoint: TrackedWaypoint,
}

#[cfg(test)]
mod tests;

packet!(AddEntity, "add_entity", Clientbound);
packet!(Animate, "animate", Clientbound);
packet!(EntitySwing, "swing_animation", Clientbound);
packet!(DamageEvent, "damage_event", Clientbound);
packet!(EntityEvent, "entity_event", Clientbound);
packet!(EntityPositionSync, "entity_position_sync", Clientbound);
packet!(HurtAnimation, "hurt_animation", Clientbound);
packet!(MoveEntityPos, "move_entity_pos", Clientbound);
packet!(MoveEntityPosRot, "move_entity_pos_rot", Clientbound);
packet!(MoveEntityRot, "move_entity_rot", Clientbound);
packet!(
    MoveMinecartAlongTrack,
    "move_minecart_along_track",
    Clientbound
);
packet!(MoveVehicle, "move_vehicle", Clientbound);
packet!(ProjectilePower, "projectile_power", Clientbound);
packet!(RemoveEntities, "remove_entities", Clientbound);
packet!(RemoveMobEffect, "remove_mob_effect", Clientbound);
packet!(RotateHead, "rotate_head", Clientbound);
packet!(SetEntityData, "set_entity_data", Clientbound);
packet!(SetEntityLink, "set_entity_link", Clientbound);
packet!(SetEntityMotion, "set_entity_motion", Clientbound);
packet!(SetEquipment, "set_equipment", Clientbound);
packet!(SetPassengers, "set_passengers", Clientbound);
packet!(TakeItemEntity, "take_item_entity", Clientbound);
packet!(TeleportEntity, "teleport_entity", Clientbound);
packet!(UpdateAttributes, "update_attributes", Clientbound);
packet!(UpdateMobEffect, "update_mob_effect", Clientbound);
packet!(Waypoint, "waypoint", Clientbound);
