use std::collections::HashMap;

use mcr_buf::{DecodeError, read_varint, write_varint};

use super::tables::{IdMap, VersionTables};
use crate::{Direction, PacketIds, ProtocolState};

const STATES: [ProtocolState; 5] = [
    ProtocolState::Handshake,
    ProtocolState::Status,
    ProtocolState::Login,
    ProtocolState::Configuration,
    ProtocolState::Play,
];
const DIRECTIONS: [Direction; 2] = [Direction::Clientbound, Direction::Serverbound];
const NO_ID: i32 = -1;

fn slot(state: ProtocolState, direction: Direction) -> usize {
    state as usize * 2 + direction as usize
}

fn state_name(state: ProtocolState) -> &'static str {
    ["handshake", "status", "login", "configuration", "play"][state as usize]
}

fn direction_name(direction: Direction) -> &'static str {
    ["clientbound", "serverbound"][direction as usize]
}

fn short(name: &str) -> &str {
    name.strip_prefix("minecraft:").unwrap_or(name)
}

#[derive(Debug, PartialEq, Eq)]
pub enum Translated<'a> {
    Packet(i32, &'a [u8]),
    /// The translated packet (if any) followed by packets the translator added.
    Packets(Frames<'a>),
    Drop,
}

/// Packets from one translation, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frames<'a> {
    first: Option<(i32, &'a [u8])>,
    /// `[VarInt id][VarInt length][payload]` each, written by the translator.
    rest: &'a [u8],
}

impl<'a> Iterator for Frames<'a> {
    type Item = (i32, &'a [u8]);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(first) = self.first.take() {
            return Some(first);
        }
        let mut r = self.rest;
        let id = read_varint(&mut r).ok()?;
        let max = r.len();
        let len = mcr_buf::read_len(&mut r, max).ok()?;
        let (payload, tail) = r.split_at(len);
        self.rest = tail;
        Some((id, payload))
    }
}

/// Per-connection translator state, shared by both directions; the canonical wire never touches it.
#[derive(Debug, Default)]
pub struct Session {
    pub(crate) state: SessionState,
    extra: Vec<u8>,
}

/// Everything a translator remembers about a connection; all of it bounded.
#[derive(Debug, Default)]
pub(crate) struct SessionState {
    /// The open brewing stand window and whether its 26.3-only totals were sent.
    pub brewing: Option<(i32, bool)>,
    /// The `minecraft:core` pack version the server offered.
    pub core_pack: Option<Box<str>>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TranslateError {
    #[error("no {state:?} {direction:?} packet with id {id} in {version}")]
    UnknownId {
        version: &'static str,
        state: ProtocolState,
        direction: Direction,
        id: i32,
    },
    #[error("translating {packet} ({state:?} {direction:?}) for {version}: {source}")]
    Packet {
        version: &'static str,
        packet: String,
        state: ProtocolState,
        direction: Direction,
        source: DecodeError,
    },
}

/// What a rewrite produced; the payload, if any, is in the output buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    /// The output is the packet's payload on the target side.
    Rewritten,
    /// The source payload goes on unchanged under the target id.
    Same,
    /// The output is the payload of another target-side packet, named without `minecraft:`.
    Replace(&'static str),
    Drop,
}

/// What a rewrite can see besides its payload.
pub(crate) struct Cx<'s> {
    pub t: &'static Translator,
    pub m: &'static Maps,
    pub session: &'s mut SessionState,
    state: ProtocolState,
    direction: Direction,
    extra: &'s mut Vec<u8>,
}

impl Cx<'_> {
    /// Adds a target-side packet after the one being translated.
    pub fn emit(
        &mut self,
        name: &'static str,
        payload: impl FnOnce(&mut Vec<u8>),
    ) -> Result<(), DecodeError> {
        let id = self
            .t
            .target_id(self.state, self.direction, name)
            .ok_or_else(|| DecodeError::Other(format!("no target packet {name}")))?;
        let mut body = Vec::new();
        payload(&mut body);
        write_varint(self.extra, id);
        write_varint(self.extra, body.len() as i32);
        self.extra.extend_from_slice(&body);
        Ok(())
    }
}

pub(crate) type Rewrite = fn(&mut Cx<'_>, &mut &[u8], &mut Vec<u8>) -> Result<Action, DecodeError>;

/// A rewrite for one packet: clientbound rules name the old packet, serverbound rules the canonical one.
#[derive(Clone, Copy)]
pub(crate) struct Rule {
    pub state: ProtocolState,
    pub direction: Direction,
    pub name: &'static str,
    pub rewrite: Rewrite,
}

#[derive(Clone, Copy)]
struct Route {
    target: i32,
    rewrite: Option<Rewrite>,
}

#[derive(Default)]
struct Routes {
    /// Ids pass unchanged and nothing is rewritten.
    passthrough: bool,
    by_source: Vec<Route>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Remap<'a> {
    Identity,
    Map(&'a IdMap),
}

impl Remap<'_> {
    /// Old id to canonical id.
    #[inline]
    pub fn forward(self, id: i32) -> Option<i32> {
        match self {
            Self::Identity => (id >= 0).then_some(id),
            Self::Map(m) => m.forward(id),
        }
    }

    /// Canonical id to old id.
    #[inline]
    pub fn back(self, id: i32) -> Option<i32> {
        match self {
            Self::Identity => (id >= 0).then_some(id),
            Self::Map(m) => m.back(id),
        }
    }
}

/// Registry maps and old-only entries the rewrites use, resolved once per translator.
#[derive(Debug)]
pub(crate) struct Maps {
    pub item: Remap<'static>,
    pub block: Remap<'static>,
    pub entity: Remap<'static>,
    pub sound: Remap<'static>,
    pub particle: Remap<'static>,
    pub component: Remap<'static>,
    pub argument: Remap<'static>,
    pub custom_stat: Remap<'static>,
    pub attribute: Remap<'static>,
    pub block_entity: Remap<'static>,
    pub menu: Remap<'static>,
    pub states: Remap<'static>,
    /// Old data component ids with no canonical entry of the same name.
    pub old_swing_animation: Option<i32>,
    pub old_map_color: Option<i32>,
    pub canonical: CanonicalIds,
}

/// Canonical registry ids the rewrites compare against, looked up by name once.
#[derive(Debug)]
pub(crate) struct CanonicalIds {
    pub brick: Option<i32>,
    pub brewing_stand_menu: Option<i32>,
    pub decorated_pot: Option<i32>,
    pub potion_contents_predicate: Option<i32>,
    pub wax_on_sound: Option<i32>,
}

impl CanonicalIds {
    fn new() -> Self {
        let id = |registry: &str, name: &str| {
            mcr_data::v26_3()
                .registry(registry)
                .and_then(|r| r.id_of(name))
                .map(|id| id as i32)
        };
        Self {
            brick: id("minecraft:item", "minecraft:brick"),
            brewing_stand_menu: id("minecraft:menu", "minecraft:brewing_stand"),
            decorated_pot: id("minecraft:block_entity_type", "minecraft:decorated_pot"),
            potion_contents_predicate: id(
                "minecraft:data_component_predicate_type",
                "minecraft:potion_contents",
            ),
            wax_on_sound: id("minecraft:sound_event", "minecraft:item.honeycomb.wax_on"),
        }
    }
}

impl Maps {
    fn new(tables: &'static VersionTables) -> Self {
        let registry = |name: &str| {
            tables
                .map(&VersionTables::registry_key(name))
                .map_or(Remap::Identity, Remap::Map)
        };
        let old_component = |name: &str| old_entry(tables, "minecraft:data_component_type", name);
        Self {
            item: registry("minecraft:item"),
            block: registry("minecraft:block"),
            entity: registry("minecraft:entity_type"),
            sound: registry("minecraft:sound_event"),
            particle: registry("minecraft:particle_type"),
            component: registry("minecraft:data_component_type"),
            argument: registry("minecraft:command_argument_type"),
            custom_stat: registry("minecraft:custom_stat"),
            attribute: registry("minecraft:attribute"),
            block_entity: registry("minecraft:block_entity_type"),
            menu: registry("minecraft:menu"),
            states: tables
                .map("block_state")
                .map_or(Remap::Identity, Remap::Map),
            old_swing_animation: old_component("minecraft:swing_animation"),
            old_map_color: old_component("minecraft:map_color"),
            canonical: CanonicalIds::new(),
        }
    }
}

fn old_entry(tables: &VersionTables, registry: &str, name: &str) -> Option<i32> {
    tables
        .old_names(&VersionTables::registry_key(registry))
        .iter()
        .position(|n| n == name)
        .map(|i| i as i32)
}

/// A registry entry renamed between an old version and the canonical one: `(registry, old, canonical)`.
pub(crate) type Rename = (&'static str, &'static str, &'static str);

fn apply_renames(tables: &mut VersionTables, renames: &[Rename]) -> Result<(), String> {
    let data = mcr_data::v26_3();
    for &(registry, old, new) in renames {
        let old_id = old_entry(tables, registry, old)
            .ok_or_else(|| format!("{registry}: no old entry {old}"))?;
        let new_id = data
            .registry(registry)
            .and_then(|r| r.id_of(new))
            .ok_or_else(|| format!("{registry}: no canonical entry {new}"))?;
        let key = VersionTables::registry_key(registry);
        let map = tables
            .maps
            .iter_mut()
            .find(|(k, _)| *k == key)
            .map(|(_, m)| m)
            .ok_or_else(|| format!("{registry}: renamed but identical"))?;
        let (o, n) = (old_id as usize, new_id as usize);
        if o >= map.forward.len() || n >= map.back.len() {
            return Err(format!("{registry}: rename out of range"));
        }
        map.forward[o] = n as u16;
        map.back[n] = o as u16;
    }
    Ok(())
}

/// Translates one older version's frames to and from the canonical packet set.
pub struct Translator {
    version: &'static str,
    protocol: i32,
    tables: &'static VersionTables,
    maps: Maps,
    routes: [Routes; 10],
    old_ids: [HashMap<String, i32>; 10],
}

impl std::fmt::Debug for Translator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Translator")
            .field("version", &self.version)
            .field("protocol", &self.protocol)
            .finish_non_exhaustive()
    }
}

impl Translator {
    /// Built once per version and kept for the process, so the tables are leaked to `'static`.
    /// Later rules for the same packet replace earlier ones.
    pub(crate) fn new(
        version: &'static str,
        tables: &[u8],
        rules: &[&[Rule]],
        renames: &[Rename],
    ) -> Result<Self, String> {
        let mut tables = VersionTables::decode(tables).map_err(|e| e.to_string())?;
        if tables.old_name != version {
            return Err(format!("tables are for {}", tables.old_name));
        }
        apply_renames(&mut tables, renames)?;
        let tables: &'static VersionTables = Box::leak(Box::new(tables));
        let canonical = PacketIds::v26_3();
        let data = mcr_data::v26_3();
        let mut routes: [Routes; 10] = Default::default();
        let mut old_ids: [HashMap<String, i32>; 10] = Default::default();
        for state in STATES {
            for direction in DIRECTIONS {
                let key = VersionTables::packet_key(state_name(state), direction_name(direction));
                let map = tables.map(&key);
                let names = tables.old_names(&key);
                let s = slot(state, direction);
                for (id, name) in names.iter().enumerate() {
                    old_ids[s].insert(short(name).to_owned(), id as i32);
                }
                let canonical_count = (0..)
                    .take_while(|&i| data.packet_name(state, direction, i).is_some())
                    .count();
                let inbound = direction == Direction::Clientbound;
                let r = &mut routes[s];
                r.by_source = match map {
                    Some(map) if inbound => map
                        .forward
                        .iter()
                        .enumerate()
                        .map(|(old, _)| Route {
                            target: map.forward(old as i32).unwrap_or(NO_ID),
                            rewrite: None,
                        })
                        .collect(),
                    Some(map) => (0..canonical_count)
                        .map(|new| Route {
                            target: map.back(new as i32).unwrap_or(NO_ID),
                            rewrite: None,
                        })
                        .collect(),
                    None => (0..canonical_count)
                        .map(|id| Route {
                            target: id as i32,
                            rewrite: None,
                        })
                        .collect(),
                };
                r.passthrough = map.is_none();
            }
        }
        for rule in rules.iter().copied().flatten() {
            let s = slot(rule.state, rule.direction);
            let name = short(rule.name);
            let source = if rule.direction == Direction::Clientbound && !routes[s].passthrough {
                old_ids[s].get(name).copied()
            } else {
                canonical.id_by_name(rule.state, rule.direction, name)
            }
            .ok_or_else(|| format!("rule for unknown packet {name}"))?;
            let route = routes[s]
                .by_source
                .get_mut(source as usize)
                .ok_or_else(|| format!("rule for unknown packet {name}"))?;
            route.rewrite = Some(rule.rewrite);
            routes[s].passthrough = false;
        }
        Ok(Self {
            version,
            protocol: tables.old_protocol as i32,
            tables,
            maps: Maps::new(tables),
            routes,
            old_ids,
        })
    }

    pub fn version_name(&self) -> &'static str {
        self.version
    }

    pub fn protocol(&self) -> i32 {
        self.protocol
    }

    #[cfg(test)]
    pub(crate) fn tables(&self) -> &VersionTables {
        self.tables
    }

    #[cfg(test)]
    pub(crate) fn maps(&self) -> &Maps {
        &self.maps
    }

    pub(crate) fn registry(&self, name: &str) -> Remap<'static> {
        self.tables
            .map(&VersionTables::registry_key(name))
            .map_or(Remap::Identity, Remap::Map)
    }

    pub(crate) fn wire_clientbound_id(&self, state: ProtocolState, canonical: i32) -> Option<i32> {
        let routes = &self.routes[slot(state, Direction::Clientbound)];
        if routes.passthrough {
            return Some(canonical);
        }
        routes
            .by_source
            .iter()
            .position(|r| r.target == canonical)
            .map(|i| i as i32)
    }

    fn target_id(&self, state: ProtocolState, direction: Direction, name: &str) -> Option<i32> {
        let s = slot(state, direction);
        if direction == Direction::Clientbound || self.routes[s].passthrough {
            PacketIds::v26_3().id_by_name(state, direction, name)
        } else {
            self.old_ids[s].get(name).copied()
        }
    }

    fn packet_name(&self, state: ProtocolState, direction: Direction, source: i32) -> String {
        let key = VersionTables::packet_key(state_name(state), direction_name(direction));
        let old = self.tables.old_names(&key);
        let name = if direction == Direction::Clientbound && !old.is_empty() {
            old.get(source as usize).map(|n| short(n).to_owned())
        } else {
            PacketIds::v26_3()
                .name_by_id(state, direction, source)
                .map(str::to_owned)
        };
        name.unwrap_or_else(|| format!("packet {source}"))
    }

    pub(crate) fn translate<'a>(
        &'static self,
        state: ProtocolState,
        direction: Direction,
        id: i32,
        payload: &'a [u8],
        session: &mut Session,
        out: &'a mut Vec<u8>,
    ) -> Result<Translated<'a>, TranslateError> {
        let routes = &self.routes[slot(state, direction)];
        if routes.passthrough {
            return Ok(Translated::Packet(id, payload));
        }
        let route = usize::try_from(id)
            .ok()
            .and_then(|i| routes.by_source.get(i))
            .ok_or(TranslateError::UnknownId {
                version: self.version,
                state,
                direction,
                id,
            })?;
        let Some(rewrite) = route.rewrite else {
            return Ok(if route.target == NO_ID {
                Translated::Drop
            } else {
                Translated::Packet(route.target, payload)
            });
        };
        out.clear();
        session.extra.clear();
        let mut input = payload;
        let fail = |source| TranslateError::Packet {
            version: self.version,
            packet: self.packet_name(state, direction, id),
            state,
            direction,
            source,
        };
        let mut cx = Cx {
            t: self,
            m: &self.maps,
            session: &mut session.state,
            state,
            direction,
            extra: &mut session.extra,
        };
        let action = rewrite(&mut cx, &mut input, out).map_err(fail)?;
        let target = match action {
            Action::Rewritten | Action::Same if route.target != NO_ID => Some(route.target),
            Action::Rewritten | Action::Same | Action::Drop => None,
            Action::Replace(name) => Some(
                self.target_id(state, direction, name)
                    .ok_or_else(|| fail(DecodeError::Other(format!("no target packet {name}"))))?,
            ),
        };
        if session.extra.is_empty() {
            return Ok(match (target, action) {
                (Some(target), Action::Same) => Translated::Packet(target, payload),
                (Some(target), _) => Translated::Packet(target, out),
                (None, _) => Translated::Drop,
            });
        }
        if action == Action::Same {
            out.clear();
            out.extend_from_slice(payload);
        }
        if target.is_none() {
            out.clear();
        }
        let main = out.len();
        out.extend_from_slice(&session.extra);
        session.extra.clear();
        let (head, rest) = out.split_at(main);
        Ok(Translated::Packets(Frames {
            first: target.map(|t| (t, head)),
            rest,
        }))
    }
}
