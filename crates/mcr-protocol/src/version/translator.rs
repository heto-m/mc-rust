use std::collections::HashMap;

use mcr_buf::DecodeError;

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
    Drop,
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
    /// The output is the payload of another target-side packet, named without `minecraft:`.
    Replace(&'static str),
    Drop,
}

pub(crate) type Rewrite = fn(&Translator, &mut &[u8], &mut Vec<u8>) -> Result<Action, DecodeError>;

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

/// Translates one older version's frames to and from the canonical packet set.
pub struct Translator {
    version: &'static str,
    protocol: i32,
    tables: VersionTables,
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
    pub(crate) fn new(
        version: &'static str,
        tables: &[u8],
        rules: &[Rule],
    ) -> Result<Self, String> {
        let tables = VersionTables::decode(tables).map_err(|e| e.to_string())?;
        if tables.old_name != version {
            return Err(format!("tables are for {}", tables.old_name));
        }
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
        for rule in rules {
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
        &self.tables
    }

    pub(crate) fn registry(&self, name: &str) -> Remap<'_> {
        self.tables
            .map(&VersionTables::registry_key(name))
            .map_or(Remap::Identity, Remap::Map)
    }

    pub(crate) fn block_states(&self) -> Remap<'_> {
        self.tables
            .map("block_state")
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
        &self,
        state: ProtocolState,
        direction: Direction,
        id: i32,
        payload: &'a [u8],
        scratch: &'a mut Vec<u8>,
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
        scratch.clear();
        let mut input = payload;
        let fail = |source| TranslateError::Packet {
            version: self.version,
            packet: self.packet_name(state, direction, id),
            state,
            direction,
            source,
        };
        match rewrite(self, &mut input, scratch).map_err(fail)? {
            Action::Rewritten if route.target != NO_ID => {
                Ok(Translated::Packet(route.target, scratch))
            }
            Action::Rewritten | Action::Drop => Ok(Translated::Drop),
            Action::Replace(name) => match self.target_id(state, direction, name) {
                Some(target) => Ok(Translated::Packet(target, scratch)),
                None => Err(fail(DecodeError::Other(format!("no target packet {name}")))),
            },
        }
    }
}
