//! 26.1.x (protocol 775) straight onto 26.3. Its layouts are 26.2's apart from the packets below, so it
//! reuses the 26.2 rewrites with its own tables.

use std::sync::OnceLock;

use mcr_buf::{Result, read_varint, take, write_varint};

use super::io;
use super::translator::{Action, Cx, Rename, Rule, Translator};
use super::v26_2::{self, entity, rule};
use crate::{Direction, ProtocolState};

static TABLES: &[u8] = include_bytes!("tables.bin");

const RENAMES: &[Rename] = &[(
    "minecraft:command_argument_type",
    "minecraft:color",
    "minecraft:team_color",
)];

pub fn translator() -> &'static Translator {
    static TRANSLATOR: OnceLock<Translator> = OnceLock::new();
    TRANSLATOR.get_or_init(|| {
        Translator::new("26.1.2", TABLES, &[v26_2::RULES, RULES], RENAMES)
            .expect("26.1.2 tables and rules agree")
    })
}

use Direction::{Clientbound as C, Serverbound as S};
use ProtocolState::{Login, Play};

static RULES: &[Rule] = &[
    rule(Login, C, "login_finished", login_finished),
    rule(Play, C, "login", login),
    rule(Play, C, "set_player_team", set_player_team),
    rule(Play, C, "update_attributes", entity::update_attributes),
    rule(Play, S, "spectator_action", spectator_action),
];

/// 26.2 added a session id for telemetry; nothing in the client reads it.
fn login_finished(_: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    io::rest(r, w);
    w.extend_from_slice(&[0; 16]);
    Ok(Action::Rewritten)
}

/// 26.1 always set up chat signing keys, which 26.2 does only when told the server is in online mode.
fn login(_: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    entity::login_head(r, w)?;
    w.push(1);
    io::bool(r, w)?;
    Ok(Action::Rewritten)
}

const TEAM_CREATE: u8 = 0;
const TEAM_UPDATE: u8 = 2;
/// `ChatFormatting` ordinals 0..=15 are the colours; the rest (formats, reset) mean no colour.
const TEAM_COLORS: i32 = 16;

/// 26.2 moved prefix and suffix after the display name, the options byte last, and made the colour an
/// optional team colour.
fn set_player_team(_: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    io::copy::<String>(r, w)?;
    let method = take(r, 1)?[0];
    w.push(method);
    if method == TEAM_CREATE || method == TEAM_UPDATE {
        io::text(r, w)?;
        let options = take(r, 1)?[0];
        let mut rules = Vec::with_capacity(4);
        io::varint(r, &mut rules)?;
        io::varint(r, &mut rules)?;
        let color = read_varint(r)?;
        io::text(r, w)?;
        io::text(r, w)?;
        w.extend_from_slice(&rules);
        if (0..TEAM_COLORS).contains(&color) {
            w.push(1);
            write_varint(w, color);
        } else {
            w.push(0);
        }
        w.push(options);
    }
    io::rest(r, w);
    Ok(Action::Rewritten)
}

/// 26.1 can only start spectating an entity; stopping is 26.2's empty action.
fn spectator_action(_: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    if !io::bool(r, &mut Vec::new())? {
        return Ok(Action::Drop);
    }
    io::varint(r, w)?;
    Ok(Action::Replace("spectate_entity"))
}

#[cfg(test)]
mod tests;
