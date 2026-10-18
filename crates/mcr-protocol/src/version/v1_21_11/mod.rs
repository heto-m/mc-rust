//! 1.21.11 (protocol 774) straight onto 26.3. Its layouts are 26.1.2's apart from the values 26.1
//! reshaped (see [`Era::Stacks`]) and the packets below, so it reuses the 26.2 and 26.1.2 rewrites.

use std::sync::OnceLock;

use mcr_buf::{Decode, IdOr, Result, read_len, read_varint, take, write_varint, write_varlong};

use super::io;
use super::translator::{Action, Cx, Era, Rule, Translator};
use super::v26_1_2;
use super::v26_2::{self, entity, rule};
use crate::play::clientbound::entity::LpVec3;
use crate::types::Vec3f;
use crate::types::component::{GlobalPos, ResolvableProfile};
use crate::types::entity_data::{
    ArmadilloState, CopperGolemState, HumanoidArm, PaintingVariantData, SnifferState, WeatherState,
};
use crate::{Direction, ProtocolState};

static TABLES: &[u8] = include_bytes!("tables.bin");

pub fn translator() -> &'static Translator {
    static TRANSLATOR: OnceLock<Translator> = OnceLock::new();
    TRANSLATOR.get_or_init(|| {
        Translator::new(
            "1.21.11",
            Era::Stacks,
            TABLES,
            &[v26_2::RULES, v26_1_2::RULES, RULES],
            v26_1_2::RENAMES,
        )
        .expect("1.21.11 tables and rules agree")
    })
}

use Direction::{Clientbound as C, Serverbound as S};
use ProtocolState::Play;

pub(super) static RULES: &[Rule] = &[
    rule(Play, C, "add_entity", add_entity),
    rule(Play, C, "login", login),
    rule(Play, C, "remove_entities", remove_entities),
    rule(Play, C, "respawn", respawn),
    rule(Play, C, "set_entity_data", set_entity_data),
    rule(Play, C, "set_time", set_time),
    rule(Play, S, "attack", attack),
    rule(Play, S, "client_command", client_command),
    rule(Play, S, "interact", interact),
    rule(Play, S, "player_input", player_input),
    rule(Play, S, "spectator_action", spectator_action),
];

/// Entity types under `AgeableMob`, whose fields from this index on moved up one when 26.1 added
/// `AGE_LOCKED` after `DATA_BABY_ID`.
const AGEABLE_SHIFT_FROM: u8 = 17;
const AGEABLE: [&str; 39] = [
    "armadillo",
    "axolotl",
    "bee",
    "camel",
    "camel_husk",
    "cat",
    "chicken",
    "cow",
    "dolphin",
    "donkey",
    "fox",
    "frog",
    "happy_ghast",
    "glow_squid",
    "goat",
    "hoglin",
    "horse",
    "llama",
    "mooshroom",
    "mule",
    "nautilus",
    "ocelot",
    "panda",
    "parrot",
    "pig",
    "polar_bear",
    "rabbit",
    "sheep",
    "skeleton_horse",
    "sniffer",
    "squid",
    "strider",
    "trader_llama",
    "turtle",
    "villager",
    "wandering_trader",
    "wolf",
    "zombie_horse",
    "zombie_nautilus",
];

/// Indexed by canonical entity type id.
fn ageable() -> &'static [bool] {
    static AGEABLE_TYPES: OnceLock<Vec<bool>> = OnceLock::new();
    AGEABLE_TYPES.get_or_init(|| {
        let data = mcr_data::v26_3();
        let mut table = Vec::new();
        for name in AGEABLE {
            let id = data
                .entity_type_by_name(&format!("minecraft:{name}"))
                .expect("canonical entity type") as usize;
            if table.len() <= id {
                table.resize(id + 1, false);
            }
            table[id] = true;
        }
        table
    })
}

fn is_ageable(kind: i32) -> bool {
    usize::try_from(kind)
        .ok()
        .and_then(|k| ageable().get(k))
        .copied()
        .unwrap_or(false)
}

fn add_entity(cx: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    let mut peek = *r;
    let id = read_varint(&mut peek)?;
    take(&mut peek, 16)?;
    let kind = cx.m.entity.forward(read_varint(&mut peek)?);
    let action = entity::add_entity(cx, r, w)?;
    if kind.is_some_and(is_ageable) {
        cx.session.track_shifted(id);
    }
    Ok(action)
}

fn remove_entities(cx: &mut Cx<'_>, r: &mut &[u8], _: &mut Vec<u8>) -> Result<Action> {
    for _ in 0..read_len(r, r.len())? {
        let id = read_varint(r)?;
        cx.session.shifted_entities.remove(&id);
    }
    Ok(Action::Same)
}

/// The client drops its entities on login and respawn; the server spawns the visible ones again.
fn login(cx: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    cx.session.shifted_entities.clear();
    v26_1_2::login(cx, r, w)
}

fn respawn(cx: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    cx.session.shifted_entities.clear();
    entity::respawn(cx, r, w)
}

/// 26.1 inserted the cat, cow, pig and chicken sound variant serializers.
fn serializer(old: i32) -> Option<i32> {
    Some(match old {
        0..=21 => old,
        22 => 23,
        23..=26 => old + 2,
        27 => 30,
        28..=38 => old + 4,
        _ => return None,
    })
}

const META_END: u8 = 0xff;
const LAST_SHARED_SERIALIZER: i32 = 21;

fn set_entity_data(cx: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    let entity_id = io::varint(r, w)?;
    let shifted = cx.session.shifted_entities.contains(&entity_id);
    loop {
        let index = take(r, 1)?[0];
        if index == META_END {
            w.push(index);
            return Ok(Action::Rewritten);
        }
        let index = if shifted && index >= AGEABLE_SHIFT_FROM {
            index
                .checked_add(1)
                .filter(|&i| i != META_END)
                .ok_or(io::unknown("entity data index", index.into()))?
        } else {
            index
        };
        w.push(index);
        let mut peek = *r;
        let old = read_varint(&mut peek)?;
        if (0..=LAST_SHARED_SERIALIZER).contains(&old) {
            entity::entity_data_value(cx.m, r, w)?;
            continue;
        }
        *r = peek;
        let new = serializer(old).ok_or(io::unknown("entity data serializer", old))?;
        write_varint(w, new);
        shifted_value(new, r, w)?;
    }
}

/// Values of the serializers whose id moved; their encodings did not change.
fn shifted_value(new: i32, r: &mut &[u8], w: &mut Vec<u8>) -> Result<()> {
    match new {
        23 | 25..=28 | 30 | 32 => io::varint(r, w).map(drop),
        33 => io::copy::<Option<GlobalPos>>(r, w),
        34 => io::copy::<IdOr<PaintingVariantData>>(r, w),
        35 => io::copy::<SnifferState>(r, w),
        36 => io::copy::<ArmadilloState>(r, w),
        37 => io::copy::<CopperGolemState>(r, w),
        38 => io::copy::<WeatherState>(r, w),
        39 => io::copy::<Vec3f>(r, w),
        40 => io::copy::<[f32; 4]>(r, w),
        41 => io::copy::<ResolvableProfile>(r, w),
        42 => io::copy::<HumanoidArm>(r, w),
        other => Err(io::unknown("entity data serializer", other)),
    }
}

/// 26.1 replaced the day time and its tick flag with per-clock states; clock 0 is the overworld's.
fn set_time(_: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    io::bytes(r, w, 8)?;
    let day = i64::from_be_bytes(take(r, 8)?.try_into().expect("8 bytes"));
    let ticking = io::bool(r, &mut Vec::new())?;
    write_varint(w, 1);
    write_varint(w, 0);
    write_varlong(w, day);
    w.extend_from_slice(&0f32.to_be_bytes());
    w.extend_from_slice(&if ticking { 1f32 } else { 0f32 }.to_be_bytes());
    Ok(Action::Rewritten)
}

const INTERACT: i32 = 0;
const ATTACK: i32 = 1;
const INTERACT_AT: i32 = 2;

/// 1.21.11 clients sent interact-at (for armor stands) and then interact; 26.1 merged them.
fn interact(cx: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    let entity = read_varint(r)?;
    let hand = read_varint(r)?;
    if !(0..=1).contains(&hand) {
        return Err(io::unknown("hand", hand));
    }
    let at = LpVec3::decode(r)?.0;
    let sneaking = io::bool(r, &mut Vec::new())?;
    write_varint(w, entity);
    write_varint(w, INTERACT_AT);
    for v in [at.x, at.y, at.z] {
        w.extend_from_slice(&(v as f32).to_be_bytes());
    }
    write_varint(w, hand);
    w.push(u8::from(sneaking));
    cx.emit("interact", |w| {
        write_varint(w, entity);
        write_varint(w, INTERACT);
        write_varint(w, hand);
        w.push(u8::from(sneaking));
    })?;
    Ok(Action::Rewritten)
}

fn attack_action(cx: &Cx<'_>, entity: i32, w: &mut Vec<u8>) -> Action {
    write_varint(w, entity);
    write_varint(w, ATTACK);
    w.push(u8::from(cx.session.sneaking));
    Action::Replace("interact")
}

fn attack(cx: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    let entity = read_varint(r)?;
    Ok(attack_action(cx, entity, w))
}

/// A 1.21.11 server spectates whatever a spectator attacks; stopping is the sneak key.
fn spectator_action(cx: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    if !io::bool(r, &mut Vec::new())? {
        return Ok(Action::Drop);
    }
    let entity = read_varint(r)?;
    Ok(attack_action(cx, entity, w))
}

const INPUT_SHIFT: u8 = 0x20;

fn player_input(cx: &mut Cx<'_>, r: &mut &[u8], _: &mut Vec<u8>) -> Result<Action> {
    cx.session.sneaking = take(r, 1)?[0] & INPUT_SHIFT != 0;
    Ok(Action::Same)
}

/// 26.1 appended `REQUEST_GAMERULE_VALUES`, which 1.21.11 servers do not know.
fn client_command(_: &mut Cx<'_>, r: &mut &[u8], _: &mut Vec<u8>) -> Result<Action> {
    Ok(match read_varint(r)? {
        0 | 1 => Action::Same,
        _ => Action::Drop,
    })
}

#[cfg(test)]
mod tests;
