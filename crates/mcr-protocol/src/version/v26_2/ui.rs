//! Containers, trades, recipes, advancements, commands, stats, tags and chat.

use mcr_buf::{
    BoundedString, IdOr, Identifier, Result, decode_str, encode_str, read_len, read_varint, take,
    write_varint,
};

use super::{items, maps, world};
use crate::play::clientbound::player::SignedMessageBody;
use crate::play::clientbound::ui::ArgumentParser;
use crate::types::Component;
use crate::types::component::{TrimPattern, component_type_id};
use crate::version::io::{self, Depth};
use crate::version::translator::{Action, Remap, Translator};

pub fn container_set_content(_: &Translator, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    io::varint(r, w)?;
    io::varint(r, w)?;
    for _ in 0..io::len(r, w, r.len())? {
        items::stack(r, w, Depth::top())?;
    }
    items::stack(r, w, Depth::top())?;
    Ok(Action::Rewritten)
}

pub fn container_set_slot(_: &Translator, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    io::varint(r, w)?;
    io::varint(r, w)?;
    io::bytes(r, w, 2)?;
    items::stack(r, w, Depth::top())?;
    Ok(Action::Rewritten)
}

pub fn set_cursor_item(_: &Translator, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    items::stack(r, w, Depth::top())?;
    Ok(Action::Rewritten)
}

pub fn set_player_inventory(_: &Translator, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    io::varint(r, w)?;
    items::stack(r, w, Depth::top())?;
    Ok(Action::Rewritten)
}

fn item_cost(r: &mut &[u8], w: &mut Vec<u8>) -> Result<()> {
    io::remap(r, w, "item", |id| maps().item.forward(id))?;
    io::varint(r, w)?;
    let n = read_len(r, r.len())?;
    let mut body = Vec::new();
    let mut count = 0;
    for _ in 0..n {
        count += items::component(r, &mut body, Depth::top())?;
    }
    write_varint(w, count);
    w.extend_from_slice(&body);
    Ok(())
}

pub fn merchant_offers(_: &Translator, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    io::varint(r, w)?;
    for _ in 0..io::len(r, w, r.len())? {
        item_cost(r, w)?;
        items::stack(r, w, Depth::top())?;
        if io::bool(r, w)? {
            item_cost(r, w)?;
        }
        io::bytes(r, w, 1 + 4 * 4 + 4 + 4)?;
    }
    io::rest(r, w);
    Ok(Action::Rewritten)
}

/// 26.3 moved an advancement's tree position out of its display, after every added entry.
pub fn update_advancements(_: &Translator, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    io::bool(r, w)?;
    for _ in 0..io::len(r, w, r.len())? {
        io::copy::<Identifier>(r, w)?;
        io::copy::<Option<Identifier>>(r, w)?;
        let mut position: &[u8] = &[0; 8];
        if io::bool(r, w)? {
            io::copy::<Component>(r, w)?;
            io::copy::<Component>(r, w)?;
            items::template(r, w, Depth::top())?;
            io::varint(r, w)?;
            let flags = i32::from_be_bytes(take(r, 4)?.try_into().expect("4 bytes"));
            w.extend_from_slice(&flags.to_be_bytes());
            if flags & 1 != 0 {
                io::copy::<Identifier>(r, w)?;
            }
            position = take(r, 8)?;
        }
        io::copy::<Vec<Vec<String>>>(r, w)?;
        io::bool(r, w)?;
        w.extend_from_slice(position);
    }
    io::rest(r, w);
    Ok(Action::Rewritten)
}

/// 26.2 tag slot displays named a tag; 26.3 takes a holder set.
fn slot_display(r: &mut &[u8], w: &mut Vec<u8>, d: Depth) -> Result<()> {
    let d = d.deeper()?;
    match io::varint(r, w)? {
        0 | 1 => {}
        2 => slot_display(r, w, d)?,
        3 => {
            slot_display(r, w, d)?;
            let old = read_varint(r)?;
            let new = match maps().component.forward(old) {
                Some(id) => id,
                None if old == 40 => component_type_id("attack_animation").expect("canonical"),
                None => return Err(io::unknown("data component type", old)),
            };
            write_varint(w, new);
        }
        4 => {
            io::remap(r, w, "item", |id| maps().item.forward(id))?;
        }
        5 => items::template(r, w, d)?,
        6 => {
            write_varint(w, 0);
            io::copy::<Identifier>(r, w)?;
        }
        7 | 9 => {
            slot_display(r, w, d)?;
            slot_display(r, w, d)?;
        }
        8 => {
            slot_display(r, w, d)?;
            slot_display(r, w, d)?;
            io::copy::<IdOr<TrimPattern>>(r, w)?;
        }
        10 => {
            for _ in 0..io::len(r, w, r.len())? {
                slot_display(r, w, d)?;
            }
        }
        other => return Err(io::unknown("slot display", other)),
    }
    Ok(())
}

fn slot_displays(r: &mut &[u8], w: &mut Vec<u8>, n: usize) -> Result<()> {
    for _ in 0..n {
        slot_display(r, w, Depth::top())?;
    }
    Ok(())
}

fn recipe_display(r: &mut &[u8], w: &mut Vec<u8>) -> Result<()> {
    match io::varint(r, w)? {
        0 => {
            let n = io::len(r, w, r.len())?;
            slot_displays(r, w, n + 2)?;
        }
        1 => {
            io::varint(r, w)?;
            io::varint(r, w)?;
            let n = io::len(r, w, r.len())?;
            slot_displays(r, w, n + 2)?;
        }
        2 => {
            slot_displays(r, w, 4)?;
            io::varint(r, w)?;
            io::bytes(r, w, 4)?;
        }
        3 => slot_displays(r, w, 3)?,
        4 => slot_displays(r, w, 5)?,
        other => return Err(io::unknown("recipe display", other)),
    }
    Ok(())
}

pub fn recipe_book_add(_: &Translator, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    for _ in 0..io::len(r, w, r.len())? {
        io::varint(r, w)?;
        recipe_display(r, w)?;
        io::varint(r, w)?;
        io::varint(r, w)?;
        if io::bool(r, w)? {
            for _ in 0..io::len(r, w, r.len())? {
                items::holder_set(r, w, maps().item, "item")?;
            }
        }
        io::bytes(r, w, 1)?;
    }
    io::bool(r, w)?;
    Ok(Action::Rewritten)
}

pub fn place_ghost_recipe(_: &Translator, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    io::varint(r, w)?;
    recipe_display(r, w)?;
    Ok(Action::Rewritten)
}

/// 26.2 hard-codes its brewing stand slot rules; 26.3 reads them from these property sets.
const BREWING_SETS: [(&str, &[&str]); 2] = [
    (
        "minecraft:brewing_input",
        &[
            "potion",
            "splash_potion",
            "lingering_potion",
            "glass_bottle",
        ],
    ),
    (
        "minecraft:brewing_reagent",
        &[
            "gunpowder",
            "dragon_breath",
            "glowstone_dust",
            "redstone",
            "nether_wart",
            "breeze_rod",
            "slime_block",
            "stone",
            "cobweb",
            "golden_carrot",
            "fermented_spider_eye",
            "magma_cream",
            "rabbit_foot",
            "turtle_helmet",
            "sugar",
            "pufferfish",
            "glistering_melon_slice",
            "spider_eye",
            "ghast_tear",
            "blaze_powder",
            "phantom_membrane",
        ],
    ),
];

pub fn update_recipes(_: &Translator, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    let sets = read_len(r, r.len())?;
    let mut body = Vec::new();
    let mut names = Vec::with_capacity(sets.min(64));
    for _ in 0..sets {
        let name = <Identifier as mcr_buf::Decode>::decode(r)?;
        mcr_buf::Encode::encode(&name, &mut body);
        names.push(name);
        ids(r, &mut body, maps().item)?;
    }
    let data = mcr_data::v26_3();
    let mut added = 0;
    for (name, items) in BREWING_SETS {
        if names.iter().any(|n| n.as_str() == name) {
            continue;
        }
        encode_str(name, &mut body);
        let ids: Vec<u32> = items
            .iter()
            .filter_map(|i| data.item_by_name(&format!("minecraft:{i}")))
            .collect();
        write_varint(&mut body, ids.len() as i32);
        for id in ids {
            write_varint(&mut body, id as i32);
        }
        added += 1;
    }
    write_varint(w, (sets + added) as i32);
    w.extend_from_slice(&body);
    for _ in 0..io::len(r, w, r.len())? {
        items::holder_set(r, w, maps().item, "item")?;
        slot_display(r, w, Depth::top())?;
    }
    Ok(Action::Rewritten)
}

/// A VarInt id list; ids with no canonical entry are left out.
fn ids(r: &mut &[u8], w: &mut Vec<u8>, map: Remap<'_>) -> Result<()> {
    let n = read_len(r, r.len())?;
    let mut kept = Vec::with_capacity(n.min(4096));
    for _ in 0..n {
        if let Some(id) = map.forward(read_varint(r)?) {
            kept.push(id);
        }
    }
    write_varint(w, kept.len() as i32);
    for id in kept {
        write_varint(w, id);
    }
    Ok(())
}

pub fn update_tags(t: &Translator, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    for _ in 0..io::len(r, w, r.len())? {
        let registry = decode_str(r, mcr_buf::MAX_STRING_CHARS)?;
        encode_str(registry, w);
        let full = Identifier::parse(registry).map_err(|_| io::unknown("registry name", 0))?;
        let map = t.registry(full.as_str());
        for _ in 0..io::len(r, w, r.len())? {
            io::copy::<Identifier>(r, w)?;
            ids(r, w, map)?;
        }
    }
    Ok(Action::Rewritten)
}

pub fn commands(_: &Translator, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    for _ in 0..io::len(r, w, r.len())? {
        let flags = take(r, 1)?[0];
        w.push(flags);
        for _ in 0..io::len(r, w, r.len())? {
            io::varint(r, w)?;
        }
        if flags & 8 != 0 {
            io::varint(r, w)?;
        }
        match flags & 3 {
            0 => {}
            1 => io::copy::<String>(r, w)?,
            2 => {
                io::copy::<String>(r, w)?;
                let new = io::remap(r, w, "argument type", |id| maps().argument.forward(id))?;
                let start = *r;
                ArgumentParser::decode_properties(new, r)?;
                w.extend_from_slice(&start[..start.len() - r.len()]);
                if flags & 16 != 0 {
                    io::copy::<Identifier>(r, w)?;
                }
            }
            other => return Err(io::unknown("command node type", other.into())),
        }
    }
    io::rest(r, w);
    Ok(Action::Rewritten)
}

pub fn award_stats(_: &Translator, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    let m = maps();
    let n = read_len(r, r.len())?;
    let mut body = Vec::new();
    let mut kept = 0;
    for _ in 0..n {
        let kind = read_varint(r)?;
        let value = read_varint(r)?;
        let count = read_varint(r)?;
        let map = match kind {
            0 => m.block,
            1..=5 => m.item,
            6 | 7 => m.entity,
            8 => m.custom_stat,
            other => return Err(io::unknown("stat type", other)),
        };
        if let Some(value) = map.forward(value) {
            write_varint(&mut body, kind);
            write_varint(&mut body, value);
            write_varint(&mut body, count);
            kept += 1;
        }
    }
    write_varint(w, kept);
    w.extend_from_slice(&body);
    Ok(Action::Rewritten)
}

pub fn player_chat(_: &Translator, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    io::varint(r, w)?;
    io::bytes(r, w, 16)?;
    io::varint(r, w)?;
    if io::bool(r, w)? {
        io::bytes(r, w, 256)?;
    }
    io::copy::<SignedMessageBody>(r, w)?;
    io::copy::<Option<Component>>(r, w)?;
    if io::varint(r, w)? == 2 {
        world::bit_set(r, w)?;
    }
    io::rest(r, w);
    Ok(Action::Rewritten)
}

/// 26.3 echoes the player's position; 26.2 wants only the teleport id.
pub fn accept_teleportation(_: &Translator, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    io::varint(r, w)?;
    *r = &[];
    Ok(Action::Rewritten)
}

/// 26.3 moved the side after the lines and made it a VarInt (front = 1).
pub fn sign_update(_: &Translator, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    let pos = take(r, 8)?;
    let mut lines = Vec::new();
    for _ in 0..4 {
        io::copy::<BoundedString<384>>(r, &mut lines)?;
    }
    let side = read_varint(r)?;
    w.extend_from_slice(pos);
    w.push(u8::from(side == 1));
    w.extend_from_slice(&lines);
    Ok(Action::Rewritten)
}

pub fn set_creative_mode_slot(_: &Translator, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    io::bytes(r, w, 2)?;
    Ok(if items::untrusted_stack_back(r, w)? {
        Action::Rewritten
    } else {
        Action::Drop
    })
}

pub fn container_click(_: &Translator, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    io::varint(r, w)?;
    io::varint(r, w)?;
    io::bytes(r, w, 3)?;
    io::varint(r, w)?;
    for _ in 0..io::len(r, w, 128)? {
        io::bytes(r, w, 2)?;
        items::hashed_stack_back(r, w)?;
    }
    items::hashed_stack_back(r, w)?;
    Ok(Action::Rewritten)
}

/// Claiming 26.2's core pack makes the server leave out vanilla registry data, which the client
/// fills from its own 26.3 copy; 26.2's data formats would not parse.
pub fn select_known_packs(t: &Translator, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    for _ in 0..io::len(r, w, 64)? {
        let namespace = decode_str(r, mcr_buf::MAX_STRING_CHARS)?;
        let id = decode_str(r, mcr_buf::MAX_STRING_CHARS)?;
        let version = decode_str(r, mcr_buf::MAX_STRING_CHARS)?;
        encode_str(namespace, w);
        encode_str(id, w);
        if (namespace, id) == ("minecraft", "core") {
            encode_str(t.version_name(), w);
        } else {
            encode_str(version, w);
        }
    }
    Ok(Action::Rewritten)
}
