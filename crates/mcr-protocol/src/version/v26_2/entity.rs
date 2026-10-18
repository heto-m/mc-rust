//! Entity spawns, metadata, equipment, movement and the player's spawn info.

use mcr_buf::{Identifier, Result, read_varint, take, write_varint};

use super::items::Codec;
use super::world;
use crate::play::clientbound::entity::LpVec3;
use crate::types::component::AttributeModifier;
use crate::types::entity_data::EntityDataValue;
use crate::version::io::{self, Depth};
use crate::version::translator::{Action, Cx};

const META_END: u8 = 0xff;
const META_ITEM_STACK: i32 = 7;
const META_BLOCK_STATE: i32 = 14;
const META_OPTIONAL_BLOCK_STATE: i32 = 15;
const META_PARTICLE: i32 = 16;
const META_PARTICLES: i32 = 17;
/// 26.3 appended `DYE_COLOR`; 26.2's serializers end before it.
const OLD_SERIALIZER_COUNT: i32 = 43;

pub fn add_entity(cx: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    io::varint(r, w)?;
    io::bytes(r, w, 16)?;
    let kind = io::remap(r, w, "entity type", |id| cx.m.entity.forward(id))?;
    io::bytes(r, w, 24)?;
    io::copy::<LpVec3>(r, w)?;
    io::bytes(r, w, 3)?;
    let data = read_varint(r)?;
    let falling_block = mcr_data::v26_3()
        .entity_type_by_name("minecraft:falling_block")
        .map(|id| id as i32);
    if Some(kind) == falling_block {
        write_varint(w, cx.m.states.forward(data).unwrap_or(0));
    } else {
        write_varint(w, data);
    }
    Ok(Action::Rewritten)
}

pub fn set_entity_data(cx: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    io::varint(r, w)?;
    loop {
        let index = take(r, 1)?[0];
        w.push(index);
        if index == META_END {
            return Ok(Action::Rewritten);
        }
        let mut peek = *r;
        let serializer = read_varint(&mut peek)?;
        match serializer {
            META_ITEM_STACK => {
                *r = peek;
                write_varint(w, serializer);
                Codec::fwd(cx.m).stack(r, w, Depth::top())?;
            }
            META_BLOCK_STATE | META_OPTIONAL_BLOCK_STATE => {
                *r = peek;
                write_varint(w, serializer);
                let old = read_varint(r)?;
                let new = if serializer == META_OPTIONAL_BLOCK_STATE && old == 0 {
                    0
                } else {
                    cx.m.states
                        .forward(old)
                        .ok_or(io::unknown("block state", old))?
                };
                write_varint(w, new);
            }
            META_PARTICLE => {
                *r = peek;
                write_varint(w, serializer);
                world::particle(cx.m, r, w, Depth::top())?;
            }
            META_PARTICLES => {
                *r = peek;
                write_varint(w, serializer);
                for _ in 0..io::len(r, w, r.len())? {
                    world::particle(cx.m, r, w, Depth::top())?;
                }
            }
            s if (0..OLD_SERIALIZER_COUNT).contains(&s) => io::copy::<EntityDataValue>(r, w)?,
            s => return Err(io::unknown("entity data serializer", s)),
        }
    }
}

pub fn set_equipment(cx: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    io::varint(r, w)?;
    loop {
        let slot = take(r, 1)?[0];
        w.push(slot);
        Codec::fwd(cx.m).stack(r, w, Depth::top())?;
        if slot & 0x80 == 0 {
            return Ok(Action::Rewritten);
        }
    }
}

/// 26.3 moved `on_ground` into a VarInt ahead of the delta (bit 0; step count 0 above it).
pub fn move_entity_pos(_: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    io::varint(r, w)?;
    let delta = take(r, 6)?;
    let on_ground = io_bool(r)?;
    write_varint(w, i32::from(on_ground));
    w.extend_from_slice(delta);
    Ok(Action::Rewritten)
}

pub fn move_entity_pos_rot(_: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    io::varint(r, w)?;
    let delta = take(r, 6)?;
    let rot = take(r, 2)?;
    let on_ground = io_bool(r)?;
    write_varint(w, i32::from(on_ground));
    w.extend_from_slice(delta);
    w.extend_from_slice(rot);
    Ok(Action::Rewritten)
}

pub fn move_entity_rot(_: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    io::varint(r, w)?;
    let rot = take(r, 2)?;
    w.push(u8::from(io_bool(r)?));
    w.extend_from_slice(rot);
    Ok(Action::Rewritten)
}

/// 26.3 sends a position path (0 = linear) and no velocity.
pub fn entity_position_sync(_: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    io::varint(r, w)?;
    write_varint(w, 0);
    io::bytes(r, w, 24)?;
    take(r, 24)?;
    io::bytes(r, w, 8)?;
    io::bool(r, w)?;
    Ok(Action::Rewritten)
}

fn io_bool(r: &mut &[u8]) -> Result<bool> {
    io::bool(r, &mut Vec::new())
}

/// Previous game type went from a signed byte (-1 = none) to an optional VarInt.
fn spawn_info(r: &mut &[u8], w: &mut Vec<u8>) -> Result<()> {
    io::varint(r, w)?;
    io::copy::<Identifier>(r, w)?;
    io::bytes(r, w, 8)?;
    let game_type = take(r, 1)?[0];
    if game_type > 3 {
        return Err(io::unknown("game type", game_type.into()));
    }
    w.push(game_type);
    match take(r, 1)?[0] as i8 {
        -1 => w.push(0),
        g @ 0..=3 => w.push(g as u8 + 1),
        g => return Err(io::unknown("previous game type", g.into())),
    }
    io::bool(r, w)?;
    io::bool(r, w)?;
    if io::bool(r, w)? {
        io::copy::<Identifier>(r, w)?;
        io::bytes(r, w, 8)?;
    }
    io::varint(r, w)?;
    io::varint(r, w)?;
    Ok(())
}

/// Everything up to and including the spawn info.
pub fn login_head(r: &mut &[u8], w: &mut Vec<u8>) -> Result<()> {
    io::bytes(r, w, 5)?;
    for _ in 0..io::len(r, w, r.len())? {
        io::copy::<Identifier>(r, w)?;
    }
    for _ in 0..3 {
        io::varint(r, w)?;
    }
    io::bytes(r, w, 3)?;
    spawn_info(r, w)
}

pub fn login(_: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    login_head(r, w)?;
    io::rest(r, w);
    Ok(Action::Rewritten)
}

/// Attribute ids shifted before 26.2 added five attributes.
pub fn update_attributes(cx: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    io::varint(r, w)?;
    for _ in 0..io::len(r, w, 128.min(r.len()))? {
        io::remap(r, w, "attribute", |id| cx.m.attribute.forward(id))?;
        io::bytes(r, w, 8)?;
        io::copy::<Vec<AttributeModifier>>(r, w)?;
    }
    Ok(Action::Rewritten)
}

pub fn respawn(_: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    spawn_info(r, w)?;
    io::rest(r, w);
    Ok(Action::Rewritten)
}
