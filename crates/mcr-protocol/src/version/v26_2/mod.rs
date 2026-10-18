//! 26.2 (protocol 776) onto 26.3. These rewrites read 26.2's layouts, which older translators share;
//! each translator brings its own tables.

pub(super) mod entity;
pub(super) mod items;
pub(super) mod ui;
pub(super) mod world;

use std::sync::OnceLock;

use mcr_buf::{Result, read_varint, take, write_varint};

use super::io;
use super::translator::{Action, Cx, Era, Rule, Translator};
use crate::{Direction, ProtocolState};

static TABLES: &[u8] = include_bytes!("tables.bin");

pub fn translator() -> &'static Translator {
    static TRANSLATOR: OnceLock<Translator> = OnceLock::new();
    TRANSLATOR.get_or_init(|| {
        Translator::new("26.2", Era::Templates, TABLES, &[RULES], &[])
            .expect("26.2 tables and rules agree")
    })
}

pub(super) const fn rule(
    state: ProtocolState,
    direction: Direction,
    name: &'static str,
    rewrite: super::translator::Rewrite,
) -> Rule {
    Rule {
        state,
        direction,
        name,
        rewrite,
    }
}

use Direction::{Clientbound as C, Serverbound as S};
use ProtocolState::{Configuration, Handshake, Play};

/// Rules for every translator whose layouts match 26.2's.
pub(super) static RULES: &[Rule] = &[
    rule(Handshake, S, "intention", intention),
    rule(
        Configuration,
        C,
        "select_known_packs",
        ui::select_known_packs_offer,
    ),
    rule(Configuration, C, "update_tags", ui::update_tags),
    rule(
        Configuration,
        S,
        "select_known_packs",
        ui::select_known_packs,
    ),
    rule(Play, C, "add_entity", entity::add_entity),
    rule(Play, C, "animate", animate),
    rule(Play, C, "award_stats", ui::award_stats),
    rule(Play, C, "block_entity_data", world::block_entity_data),
    rule(Play, C, "block_event", world::block_event),
    rule(Play, C, "block_update", world::block_update),
    rule(Play, C, "commands", ui::commands),
    rule(Play, C, "container_set_content", ui::container_set_content),
    rule(Play, C, "container_set_data", ui::container_set_data),
    rule(Play, C, "container_set_slot", ui::container_set_slot),
    rule(
        Play,
        C,
        "entity_position_sync",
        entity::entity_position_sync,
    ),
    rule(Play, C, "explode", world::explode),
    rule(
        Play,
        C,
        "level_chunk_with_light",
        world::level_chunk_with_light,
    ),
    rule(Play, C, "level_event", world::level_event),
    rule(Play, C, "level_particles", world::level_particles),
    rule(Play, C, "light_update", world::light_update),
    rule(Play, C, "login", entity::login),
    rule(Play, C, "merchant_offers", ui::merchant_offers),
    rule(Play, C, "move_entity_pos", entity::move_entity_pos),
    rule(Play, C, "move_entity_pos_rot", entity::move_entity_pos_rot),
    rule(Play, C, "move_entity_rot", entity::move_entity_rot),
    rule(Play, C, "open_screen", ui::open_screen),
    rule(Play, C, "place_ghost_recipe", ui::place_ghost_recipe),
    rule(Play, C, "player_chat", ui::player_chat),
    rule(Play, C, "recipe_book_add", ui::recipe_book_add),
    rule(Play, C, "respawn", entity::respawn),
    rule(
        Play,
        C,
        "section_blocks_update",
        world::section_blocks_update,
    ),
    rule(Play, C, "set_cursor_item", ui::set_cursor_item),
    rule(Play, C, "set_entity_data", entity::set_entity_data),
    rule(Play, C, "set_equipment", entity::set_equipment),
    rule(Play, C, "set_player_inventory", ui::set_player_inventory),
    rule(Play, C, "sound", world::sound),
    rule(Play, C, "sound_entity", world::sound),
    rule(Play, C, "update_advancements", ui::update_advancements),
    rule(Play, C, "update_recipes", ui::update_recipes),
    rule(Play, C, "update_tags", ui::update_tags),
    rule(Play, S, "accept_teleportation", ui::accept_teleportation),
    rule(Play, S, "container_click", ui::container_click),
    rule(Play, S, "player_action", player_action),
    rule(Play, S, "punch", punch),
    rule(
        Play,
        S,
        "set_creative_mode_slot",
        ui::set_creative_mode_slot,
    ),
    rule(Play, S, "sign_update", ui::sign_update),
];

fn intention(cx: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    read_varint(r)?;
    write_varint(w, cx.t.protocol());
    io::rest(r, w);
    Ok(Action::Rewritten)
}

/// 26.3 moved swings to `swing_animation` and renumbered the rest.
fn animate(_: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    let entity = read_varint(r)?;
    let action = take(r, 1)?[0];
    write_varint(w, entity);
    let swing = |w: &mut Vec<u8>, hand: u8| {
        // 26.3's default swing: whack for 6 ticks.
        w.extend_from_slice(&[hand, 1, 6]);
        Ok(Action::Replace("swing_animation"))
    };
    match action {
        0 => swing(w, 0),
        3 => swing(w, 1),
        2 => {
            w.push(0);
            Ok(Action::Rewritten)
        }
        4 | 5 => {
            w.push(action - 3);
            Ok(Action::Rewritten)
        }
        _ => Ok(Action::Drop),
    }
}

/// 26.3 inserted `CHANGE_DESTROY_DIRECTION` at ordinal 1.
fn player_action(_: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    match read_varint(r)? {
        1 => Ok(Action::Drop),
        0 => {
            write_varint(w, 0);
            io::rest(r, w);
            Ok(Action::Rewritten)
        }
        n => {
            write_varint(w, n - 1);
            io::rest(r, w);
            Ok(Action::Rewritten)
        }
    }
}

/// 26.2 has no `punch`; its `swing` with the main hand is what the server answers with a swing.
fn punch(_: &mut Cx<'_>, _: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    write_varint(w, 0);
    Ok(Action::Replace("swing"))
}

#[cfg(test)]
mod tests;
