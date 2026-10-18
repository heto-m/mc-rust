//! Item stacks and their data components.

use mcr_buf::{
    DecodeError, IdOr, Identifier, Result, decode_str, encode_str, read_len, read_varint, take,
    write_varint,
};
use mcr_nbt::{NetworkCompound, NetworkNbt};

use super::maps;
use crate::types::component::{
    COMPONENT_NAMES, DamageReduction, HolderSet, ItemDamageFunction, KineticCondition,
    MobEffectInstance, StatePropertiesPredicate, TrimPattern, component_type_id, skip_value,
};
use crate::types::{Component, SoundEvent};
use crate::version::io::{self, Depth};
use crate::version::translator::Remap;

/// 26.2 component types with no 26.3 entry of the same name.
const OLD_SWING_ANIMATION: i32 = 40;
const OLD_MAP_COLOR: i32 = 45;

fn new_component(name: &str) -> i32 {
    component_type_id(name).expect("canonical component")
}

/// 26.3 component ids an old component id becomes; `swing_animation` split into two, `map_color` is gone.
fn component_ids(old: i32) -> Result<([i32; 2], usize)> {
    Ok(match old {
        OLD_SWING_ANIMATION => (
            [
                new_component("attack_animation"),
                new_component("interact_animation"),
            ],
            2,
        ),
        OLD_MAP_COLOR => ([0; 2], 0),
        _ => (
            [
                maps()
                    .component
                    .forward(old)
                    .ok_or(io::unknown("data component type", old))?,
                0,
            ],
            1,
        ),
    })
}

fn item(r: &mut &[u8], w: &mut Vec<u8>) -> Result<i32> {
    io::remap(r, w, "item", |id| maps().item.forward(id))
}

/// Optional stack: a count of zero or less carries nothing else.
pub fn stack(r: &mut &[u8], w: &mut Vec<u8>, d: Depth) -> Result<()> {
    let count = io::varint(r, w)?;
    if count <= 0 {
        return Ok(());
    }
    item(r, w)?;
    patch(r, w, d)
}

pub fn template(r: &mut &[u8], w: &mut Vec<u8>, d: Depth) -> Result<()> {
    item(r, w)?;
    io::varint(r, w)?;
    patch(r, w, d)
}

pub fn patch(r: &mut &[u8], w: &mut Vec<u8>, d: Depth) -> Result<()> {
    let d = d.deeper()?;
    let added = read_len(r, r.len())?;
    let removed = read_len(r, r.len())?;
    if added == 0 && removed == 0 {
        w.extend_from_slice(&[0, 0]);
        return Ok(());
    }
    let mut body = Vec::new();
    let mut new_added = 0;
    for _ in 0..added {
        new_added += component(r, &mut body, d)?;
    }
    let mut new_removed = 0;
    for _ in 0..removed {
        let (ids, n) = component_ids(read_varint(r)?)?;
        for &id in &ids[..n] {
            write_varint(&mut body, id);
            new_removed += 1;
        }
    }
    write_varint(w, new_added);
    write_varint(w, new_removed);
    w.extend_from_slice(&body);
    Ok(())
}

/// One type id and value; returns how many 26.3 components it became.
pub fn component(r: &mut &[u8], w: &mut Vec<u8>, d: Depth) -> Result<i32> {
    let old = read_varint(r)?;
    match old {
        OLD_SWING_ANIMATION => {
            let start = *r;
            read_varint(r)?;
            read_varint(r)?;
            let value = &start[..start.len() - r.len()];
            for name in ["attack_animation", "interact_animation"] {
                write_varint(w, new_component(name));
                w.extend_from_slice(value);
            }
            Ok(2)
        }
        OLD_MAP_COLOR => {
            take(r, 4)?;
            Ok(0)
        }
        _ => {
            let new = component_ids(old)?.0[0];
            write_varint(w, new);
            value(new, r, w, d)?;
            Ok(1)
        }
    }
}

fn templates(r: &mut &[u8], w: &mut Vec<u8>, max: usize, d: Depth) -> Result<()> {
    for _ in 0..io::len(r, w, max.min(r.len()))? {
        template(r, w, d)?;
    }
    Ok(())
}

fn value(new: i32, r: &mut &[u8], w: &mut Vec<u8>, d: Depth) -> Result<()> {
    let m = maps();
    match COMPONENT_NAMES[new as usize] {
        "container" => {
            for _ in 0..io::len(r, w, 256)? {
                if io::bool(r, w)? {
                    template(r, w, d)?;
                }
            }
        }
        "bundle_contents" => templates(r, w, usize::MAX, d)?,
        "charged_projectiles" => templates(r, w, 1024, d)?,
        "use_remainder" | "sulfur_cube_content" => template(r, w, d)?,
        "pot_decorations" => pot_decorations(r, w)?,
        "tooltip_display" => {
            io::bool(r, w)?;
            let n = read_len(r, r.len())?;
            let mut ids = Vec::with_capacity(n.min(256));
            for _ in 0..n {
                let (new, k) = component_ids(read_varint(r)?)?;
                ids.extend_from_slice(&new[..k]);
            }
            write_varint(w, ids.len() as i32);
            for id in ids {
                write_varint(w, id);
            }
        }
        "repairable" => holder_set(r, w, m.item, "item")?,
        "tool" => {
            for _ in 0..io::len(r, w, r.len())? {
                holder_set(r, w, m.block, "block")?;
                io::copy::<Option<f32>>(r, w)?;
                io::copy::<Option<bool>>(r, w)?;
            }
            io::copy::<f32>(r, w)?;
            io::varint(r, w)?;
            io::bool(r, w)?;
        }
        "entity_data" => typed_entity(r, w)?,
        "bees" => {
            for _ in 0..io::len(r, w, r.len())? {
                typed_entity(r, w)?;
                io::varint(r, w)?;
                io::varint(r, w)?;
            }
        }
        "break_sound" => sound(r, w)?,
        "equippable" => {
            io::varint(r, w)?;
            sound(r, w)?;
            io::copy::<Option<Identifier>>(r, w)?;
            io::copy::<Option<Identifier>>(r, w)?;
            if io::bool(r, w)? {
                holder_set(r, w, m.entity, "entity type")?;
            }
            for _ in 0..5 {
                io::bool(r, w)?;
            }
            sound(r, w)?;
        }
        "consumable" => {
            io::copy::<f32>(r, w)?;
            io::varint(r, w)?;
            sound(r, w)?;
            io::bool(r, w)?;
            consume_effects(r, w)?;
        }
        "death_protection" => consume_effects(r, w)?,
        "blocks_attacks" => {
            io::copy::<f32>(r, w)?;
            io::copy::<f32>(r, w)?;
            io::copy::<Vec<DamageReduction>>(r, w)?;
            io::copy::<ItemDamageFunction>(r, w)?;
            io::copy::<Option<HolderSet>>(r, w)?;
            optional_sound(r, w)?;
            optional_sound(r, w)?;
        }
        "piercing_weapon" => {
            io::bool(r, w)?;
            io::bool(r, w)?;
            optional_sound(r, w)?;
            optional_sound(r, w)?;
        }
        "kinetic_weapon" => {
            io::varint(r, w)?;
            io::varint(r, w)?;
            for _ in 0..3 {
                io::copy::<Option<KineticCondition>>(r, w)?;
            }
            io::copy::<f32>(r, w)?;
            io::copy::<f32>(r, w)?;
            optional_sound(r, w)?;
            optional_sound(r, w)?;
        }
        "instrument" => {
            if io::varint(r, w)? == 0 {
                sound(r, w)?;
                io::bytes(r, w, 8)?;
                // 26.3 added durability damage; 26.2 instruments never damage the item.
                write_varint(w, 0);
                io::copy::<Component>(r, w)?;
            }
        }
        "jukebox_playable" => {
            if io::varint(r, w)? == 0 {
                sound(r, w)?;
                io::copy::<Component>(r, w)?;
                io::copy::<f32>(r, w)?;
                io::varint(r, w)?;
            }
        }
        "trim" => {
            trim_material(r, w)?;
            io::copy::<IdOr<TrimPattern>>(r, w)?;
        }
        "provides_trim_material" => trim_material(r, w)?,
        "can_place_on" | "can_break" => {
            for _ in 0..io::len(r, w, r.len())? {
                block_predicate(r, w, d)?;
            }
        }
        _ => {
            let start = *r;
            skip_value(new, r)?;
            w.extend_from_slice(&start[..start.len() - r.len()]);
        }
    }
    Ok(())
}

/// 26.2 lists up to four sherd item ids (brick = blank); 26.3 has four optional stacks.
fn pot_decorations(r: &mut &[u8], w: &mut Vec<u8>) -> Result<()> {
    let n = read_len(r, 4)?;
    let mut sides = [None; 4];
    for side in sides.iter_mut().take(n) {
        let old = read_varint(r)?;
        *side = Some(maps().item.forward(old).ok_or(io::unknown("item", old))?);
    }
    let brick = mcr_data::v26_3()
        .item_by_name("minecraft:brick")
        .map(|b| b as i32);
    for side in sides {
        match side.filter(|&id| Some(id) != brick) {
            Some(id) => {
                w.push(1);
                write_varint(w, id);
                w.extend_from_slice(&[1, 0, 0]);
            }
            None => w.push(0),
        }
    }
    Ok(())
}

/// Inline 26.2 materials name an asset suffix and per-armour overrides; 26.3 names a palette.
fn trim_material(r: &mut &[u8], w: &mut Vec<u8>) -> Result<()> {
    if io::varint(r, w)? != 0 {
        return Ok(());
    }
    let base = decode_str(r, mcr_buf::MAX_STRING_CHARS)?.to_owned();
    for _ in 0..read_len(r, r.len())? {
        <Identifier as mcr_buf::Decode>::decode(r)?;
        decode_str(r, mcr_buf::MAX_STRING_CHARS)?;
    }
    let palette = Identifier::parse(&format!("minecraft:trim/{base}"))
        .map_err(|_| DecodeError::Other("trim material asset name".into()))?;
    encode_str(palette.as_str(), w);
    io::copy::<Component>(r, w)
}

fn block_predicate(r: &mut &[u8], w: &mut Vec<u8>, d: Depth) -> Result<()> {
    if io::bool(r, w)? {
        holder_set(r, w, maps().block, "block")?;
    }
    io::copy::<Option<StatePropertiesPredicate>>(r, w)?;
    io::copy::<Option<NetworkCompound>>(r, w)?;
    let exact = read_len(r, r.len())?;
    let mut body = Vec::new();
    let mut count = 0;
    for _ in 0..exact {
        count += component(r, &mut body, d)?;
    }
    write_varint(w, count);
    w.extend_from_slice(&body);
    for _ in 0..io::len(r, w, 64)? {
        if io::bool(r, w)? {
            io::varint(r, w)?;
        } else {
            let old = read_varint(r)?;
            let (ids, n) = component_ids(old)?;
            if n == 0 {
                return Err(io::unknown("component predicate type", old));
            }
            write_varint(w, ids[0]);
        }
        io::copy::<NetworkNbt>(r, w)?;
    }
    Ok(())
}

pub fn holder_set(
    r: &mut &[u8],
    w: &mut Vec<u8>,
    map: Remap<'_>,
    what: &'static str,
) -> Result<()> {
    let n = read_len(r, r.len() + 1)?;
    write_varint(w, n as i32);
    if n == 0 {
        return io::copy::<Identifier>(r, w);
    }
    for _ in 1..n {
        io::remap(r, w, what, |id| map.forward(id))?;
    }
    Ok(())
}

fn typed_entity(r: &mut &[u8], w: &mut Vec<u8>) -> Result<()> {
    io::remap(r, w, "entity type", |id| maps().entity.forward(id))?;
    io::copy::<NetworkCompound>(r, w)
}

/// `Holder<SoundEvent>`: registry id + 1, or 0 and an inline event.
pub fn sound(r: &mut &[u8], w: &mut Vec<u8>) -> Result<()> {
    let v = read_varint(r)?;
    if v == 0 {
        write_varint(w, 0);
        return io::copy::<SoundEvent>(r, w);
    }
    let new = maps()
        .sound
        .forward(v - 1)
        .ok_or(io::unknown("sound event", v - 1))?;
    write_varint(w, new + 1);
    Ok(())
}

fn optional_sound(r: &mut &[u8], w: &mut Vec<u8>) -> Result<()> {
    if io::bool(r, w)? {
        sound(r, w)?;
    }
    Ok(())
}

fn consume_effects(r: &mut &[u8], w: &mut Vec<u8>) -> Result<()> {
    for _ in 0..io::len(r, w, r.len())? {
        match io::varint(r, w)? {
            0 => {
                io::copy::<Vec<MobEffectInstance>>(r, w)?;
                io::copy::<f32>(r, w)?;
            }
            1 => io::copy::<HolderSet>(r, w)?,
            2 => {}
            3 => {
                io::copy::<f32>(r, w)?;
                // 26.3's default: particles follow the teleport direction.
                w.push(1);
            }
            4 => sound(r, w)?,
            other => return Err(io::unknown("consume effect", other)),
        }
    }
    Ok(())
}

/// Components whose values hold registry ids that differ between the versions; dropped going out.
const REMAPPED_VALUES: &[&str] = &[
    "container",
    "bundle_contents",
    "charged_projectiles",
    "use_remainder",
    "sulfur_cube_content",
    "pot_decorations",
    "tooltip_display",
    "repairable",
    "tool",
    "entity_data",
    "bees",
    "break_sound",
    "equippable",
    "consumable",
    "death_protection",
    "blocks_attacks",
    "piercing_weapon",
    "kinetic_weapon",
    "instrument",
    "jukebox_playable",
    "trim",
    "provides_trim_material",
    "can_place_on",
    "can_break",
];

/// The 26.2 id of a canonical component that can travel unchanged.
fn old_component(new: i32) -> Option<i32> {
    let name = *COMPONENT_NAMES.get(usize::try_from(new).ok()?)?;
    if name == "attack_animation" {
        return Some(OLD_SWING_ANIMATION);
    }
    if REMAPPED_VALUES.contains(&name) {
        return None;
    }
    maps().component.back(new)
}

/// Client stack with length-prefixed values, canonical to 26.2; `false` when the item has no 26.2 id.
pub fn untrusted_stack_back(r: &mut &[u8], w: &mut Vec<u8>) -> Result<bool> {
    let count = io::varint(r, w)?;
    if count <= 0 {
        return Ok(true);
    }
    let new = read_varint(r)?;
    let Some(old) = maps().item.back(new) else {
        return Ok(false);
    };
    write_varint(w, old);
    let added = read_len(r, r.len())?;
    let removed = read_len(r, r.len())?;
    let mut body = Vec::new();
    let mut kept_added = 0;
    for _ in 0..added {
        let id = read_varint(r)?;
        let len = read_len(r, r.len())?;
        let value = take(r, len)?;
        if let Some(old) = old_component(id) {
            write_varint(&mut body, old);
            write_varint(&mut body, len as i32);
            body.extend_from_slice(value);
            kept_added += 1;
        }
    }
    let mut kept_removed = 0;
    for _ in 0..removed {
        let id = read_varint(r)?;
        if let Some(old) = old_component(id) {
            write_varint(&mut body, old);
            kept_removed += 1;
        }
    }
    write_varint(w, kept_added);
    write_varint(w, kept_removed);
    w.extend_from_slice(&body);
    Ok(true)
}

/// A hashed stack from a container click, canonical to 26.2. Unknown items and components are
/// left out; the server then resends the slot, as it does after any hash mismatch.
pub fn hashed_stack_back(r: &mut &[u8], w: &mut Vec<u8>) -> Result<()> {
    if take(r, 1)?[0] == 0 {
        w.push(0);
        return Ok(());
    }
    let new = read_varint(r)?;
    let count = read_varint(r)?;
    let mut body = Vec::new();
    for removed in [false, true] {
        let n = read_len(r, 256)?;
        let mut kept = Vec::new();
        for _ in 0..n {
            let id = read_varint(r)?;
            let hash = if removed { None } else { Some(take(r, 4)?) };
            let old = if COMPONENT_NAMES.get(id as usize) == Some(&"attack_animation") {
                Some(OLD_SWING_ANIMATION)
            } else {
                maps().component.back(id)
            };
            if let Some(old) = old {
                kept.push((old, hash));
            }
        }
        write_varint(&mut body, kept.len() as i32);
        for (id, hash) in kept {
            write_varint(&mut body, id);
            if let Some(hash) = hash {
                body.extend_from_slice(hash);
            }
        }
    }
    match maps().item.back(new) {
        Some(old) => {
            w.push(1);
            write_varint(w, old);
            write_varint(w, count);
            w.extend_from_slice(&body);
        }
        None => w.push(0),
    }
    Ok(())
}

#[cfg(test)]
pub(super) mod old_ids {
    pub const SWING: i32 = super::OLD_SWING_ANIMATION;
    pub const MAP_COLOR: i32 = super::OLD_MAP_COLOR;
    pub const CONTAINER: i32 = 75;
}
