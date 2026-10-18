//! Containers, trades, recipes, advancements, commands, stats, tags and chat.

use mcr_buf::{
    BoundedString, Encode, Identifier, Result, decode_str, encode_str, read_len, read_varint, take,
    write_varint,
};

use super::items::Codec;
use super::world;
use crate::play::clientbound::player::SignedMessageBody;
use crate::play::clientbound::ui::{ArgumentParser, ContainerSetData};
use crate::types::component::component_type_id;
use crate::version::io::{self, Depth};
use crate::version::translator::{Action, Cx, Maps, Remap};

pub fn container_set_content(cx: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    let c = Codec::fwd(cx.m);
    io::varint(r, w)?;
    io::varint(r, w)?;
    for _ in 0..io::len(r, w, r.len())? {
        c.stack(r, w, Depth::top())?;
    }
    c.stack(r, w, Depth::top())?;
    Ok(Action::Rewritten)
}

pub fn container_set_slot(cx: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    io::varint(r, w)?;
    io::varint(r, w)?;
    io::bytes(r, w, 2)?;
    Codec::fwd(cx.m).stack(r, w, Depth::top())?;
    Ok(Action::Rewritten)
}

pub fn set_cursor_item(cx: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    Codec::fwd(cx.m).stack(r, w, Depth::top())?;
    Ok(Action::Rewritten)
}

pub fn set_player_inventory(cx: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    io::varint(r, w)?;
    Codec::fwd(cx.m).stack(r, w, Depth::top())?;
    Ok(Action::Rewritten)
}

/// Remembers a brewing stand window: 26.3 has two more data values for it.
pub fn open_screen(cx: &mut Cx<'_>, r: &mut &[u8], _: &mut Vec<u8>) -> Result<Action> {
    let window = read_varint(r)?;
    let menu = read_varint(r)?;
    let brewing = cx.m.canonical.brewing_stand_menu;
    cx.session.brewing =
        (brewing.is_some() && cx.m.menu.forward(menu) == brewing).then_some((window, false));
    Ok(Action::Same)
}

/// 26.2 brews for a fixed 400 ticks on 20 fuel uses; 26.3 sends both totals as data values 2 and 3.
pub const BREWING_TOTALS: [(i16, i16); 2] = [(2, 400), (3, 20)];

pub fn container_set_data(cx: &mut Cx<'_>, r: &mut &[u8], _: &mut Vec<u8>) -> Result<Action> {
    let window = read_varint(r)?;
    take(r, 4)?;
    if cx.session.brewing == Some((window, false)) {
        cx.session.brewing = Some((window, true));
        for (id, value) in BREWING_TOTALS {
            cx.emit("container_set_data", |w| {
                ContainerSetData {
                    container_id: window,
                    id,
                    value,
                }
                .encode(w)
            })?;
        }
    }
    Ok(Action::Same)
}

fn item_cost(m: &'static Maps, r: &mut &[u8], w: &mut Vec<u8>) -> Result<()> {
    let c = Codec::fwd(m);
    io::remap(r, w, "item", |id| m.item.forward(id))?;
    io::varint(r, w)?;
    let n = read_len(r, r.len())?;
    let mut body = Vec::new();
    let mut count = 0;
    for _ in 0..n {
        count += c.component(r, &mut body, Depth::top())?;
    }
    write_varint(w, count);
    w.extend_from_slice(&body);
    Ok(())
}

pub fn merchant_offers(cx: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    io::varint(r, w)?;
    for _ in 0..io::len(r, w, r.len())? {
        item_cost(cx.m, r, w)?;
        Codec::fwd(cx.m).stack(r, w, Depth::top())?;
        if io::bool(r, w)? {
            item_cost(cx.m, r, w)?;
        }
        io::bytes(r, w, 1 + 4 * 4 + 4 + 4)?;
    }
    io::rest(r, w);
    Ok(Action::Rewritten)
}

/// 26.3 moved an advancement's tree position out of its display, after every added entry.
pub fn update_advancements(cx: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    io::bool(r, w)?;
    for _ in 0..io::len(r, w, r.len())? {
        io::copy::<Identifier>(r, w)?;
        io::copy::<Option<Identifier>>(r, w)?;
        let mut position: &[u8] = &[0; 8];
        if io::bool(r, w)? {
            io::text(r, w)?;
            io::text(r, w)?;
            Codec::fwd(cx.m).template(r, w, Depth::top())?;
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
fn slot_display(c: Codec, r: &mut &[u8], w: &mut Vec<u8>, d: Depth) -> Result<()> {
    let d = d.deeper()?;
    match io::varint(r, w)? {
        0 | 1 => {}
        2 => slot_display(c, r, w, d)?,
        3 => {
            slot_display(c, r, w, d)?;
            let old = read_varint(r)?;
            let new = match c.m.component.forward(old) {
                Some(id) => id,
                None if Some(old) == c.m.old_swing_animation => {
                    component_type_id("attack_animation").expect("canonical")
                }
                None => return Err(io::unknown("data component type", old)),
            };
            write_varint(w, new);
        }
        4 => {
            io::remap(r, w, "item", |id| c.m.item.forward(id))?;
        }
        5 => c.template(r, w, d)?,
        6 => {
            write_varint(w, 0);
            io::copy::<Identifier>(r, w)?;
        }
        7 | 9 => {
            slot_display(c, r, w, d)?;
            slot_display(c, r, w, d)?;
        }
        8 => {
            slot_display(c, r, w, d)?;
            slot_display(c, r, w, d)?;
            if io::varint(r, w)? == 0 {
                io::copy::<Identifier>(r, w)?;
                io::text(r, w)?;
                io::bool(r, w)?;
            }
        }
        10 => {
            for _ in 0..io::len(r, w, r.len())? {
                slot_display(c, r, w, d)?;
            }
        }
        other => return Err(io::unknown("slot display", other)),
    }
    Ok(())
}

fn slot_displays(c: Codec, r: &mut &[u8], w: &mut Vec<u8>, n: usize) -> Result<()> {
    for _ in 0..n {
        slot_display(c, r, w, Depth::top())?;
    }
    Ok(())
}

fn recipe_display(c: Codec, r: &mut &[u8], w: &mut Vec<u8>) -> Result<()> {
    match io::varint(r, w)? {
        0 => {
            let n = io::len(r, w, r.len())?;
            slot_displays(c, r, w, n + 2)?;
        }
        1 => {
            io::varint(r, w)?;
            io::varint(r, w)?;
            let n = io::len(r, w, r.len())?;
            slot_displays(c, r, w, n + 2)?;
        }
        2 => {
            slot_displays(c, r, w, 4)?;
            io::varint(r, w)?;
            io::bytes(r, w, 4)?;
        }
        3 => slot_displays(c, r, w, 3)?,
        4 => slot_displays(c, r, w, 5)?,
        other => return Err(io::unknown("recipe display", other)),
    }
    Ok(())
}

pub fn recipe_book_add(cx: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    let c = Codec::fwd(cx.m);
    for _ in 0..io::len(r, w, r.len())? {
        io::varint(r, w)?;
        recipe_display(c, r, w)?;
        io::varint(r, w)?;
        io::varint(r, w)?;
        if io::bool(r, w)? {
            for _ in 0..io::len(r, w, r.len())? {
                c.holder_set(r, w, cx.m.item, "item")?;
            }
        }
        io::bytes(r, w, 1)?;
    }
    io::bool(r, w)?;
    Ok(Action::Rewritten)
}

pub fn place_ghost_recipe(cx: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    io::varint(r, w)?;
    recipe_display(Codec::fwd(cx.m), r, w)?;
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

pub fn update_recipes(cx: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    let c = Codec::fwd(cx.m);
    let sets = read_len(r, r.len())?;
    let mut body = Vec::new();
    let mut names = Vec::with_capacity(sets.min(64));
    for _ in 0..sets {
        let name = <Identifier as mcr_buf::Decode>::decode(r)?;
        mcr_buf::Encode::encode(&name, &mut body);
        names.push(name);
        ids(r, &mut body, cx.m.item)?;
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
        c.holder_set(r, w, cx.m.item, "item")?;
        slot_display(c, r, w, Depth::top())?;
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

pub fn update_tags(cx: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    for _ in 0..io::len(r, w, r.len())? {
        let registry = decode_str(r, mcr_buf::MAX_STRING_CHARS)?;
        encode_str(registry, w);
        let full = Identifier::parse(registry).map_err(|_| io::unknown("registry name", 0))?;
        let map = cx.t.registry(full.as_str());
        for _ in 0..io::len(r, w, r.len())? {
            io::copy::<Identifier>(r, w)?;
            ids(r, w, map)?;
        }
    }
    Ok(Action::Rewritten)
}

pub fn commands(cx: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
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
                let new = io::remap(r, w, "argument type", |id| cx.m.argument.forward(id))?;
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

pub fn award_stats(cx: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    let m = cx.m;
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

pub fn player_chat(_: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    io::varint(r, w)?;
    io::bytes(r, w, 16)?;
    io::varint(r, w)?;
    if io::bool(r, w)? {
        io::bytes(r, w, 256)?;
    }
    io::copy::<SignedMessageBody>(r, w)?;
    if io::bool(r, w)? {
        io::text(r, w)?;
    }
    if io::varint(r, w)? == 2 {
        world::bit_set(r, w)?;
    }
    io::rest(r, w);
    Ok(Action::Rewritten)
}

/// 26.3 echoes the player's position; 26.2 wants only the teleport id.
pub fn accept_teleportation(_: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    io::varint(r, w)?;
    *r = &[];
    Ok(Action::Rewritten)
}

/// 26.3 moved the side after the lines and made it a VarInt (front = 1).
pub fn sign_update(_: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
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

pub fn set_creative_mode_slot(cx: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    io::bytes(r, w, 2)?;
    Ok(if Codec::back(cx.m).untrusted_stack_back(r, w)? {
        Action::Rewritten
    } else {
        Action::Drop
    })
}

pub fn container_click(cx: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    let c = Codec::back(cx.m);
    io::varint(r, w)?;
    io::varint(r, w)?;
    io::bytes(r, w, 3)?;
    io::varint(r, w)?;
    for _ in 0..io::len(r, w, 128)? {
        io::bytes(r, w, 2)?;
        c.hashed_stack_back(r, w)?;
    }
    c.hashed_stack_back(r, w)?;
    Ok(Action::Rewritten)
}

const MAX_PACK_VERSION: usize = 64;

/// Remembers the core pack version the server offers, for [`select_known_packs`].
pub fn select_known_packs_offer(cx: &mut Cx<'_>, r: &mut &[u8], _: &mut Vec<u8>) -> Result<Action> {
    for _ in 0..read_len(r, 64.min(r.len()))? {
        let namespace = decode_str(r, mcr_buf::MAX_STRING_CHARS)?;
        let id = decode_str(r, mcr_buf::MAX_STRING_CHARS)?;
        let version = decode_str(r, MAX_PACK_VERSION)?;
        if (namespace, id) == ("minecraft", "core") {
            cx.session.core_pack = Some(version.into());
        }
    }
    Ok(Action::Same)
}

/// Claiming the old core pack makes the server leave out vanilla registry data, which the client
/// fills from its own 26.3 copy; the old data formats would not parse.
pub fn select_known_packs(cx: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    for _ in 0..io::len(r, w, 64)? {
        let namespace = decode_str(r, mcr_buf::MAX_STRING_CHARS)?;
        let id = decode_str(r, mcr_buf::MAX_STRING_CHARS)?;
        let version = decode_str(r, mcr_buf::MAX_STRING_CHARS)?;
        encode_str(namespace, w);
        encode_str(id, w);
        if (namespace, id) == ("minecraft", "core") {
            let offered = cx.session.core_pack.as_deref();
            encode_str(offered.unwrap_or(cx.t.version_name()), w);
        } else {
            encode_str(version, w);
        }
    }
    Ok(Action::Rewritten)
}
