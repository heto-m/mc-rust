use std::collections::HashMap;
use std::sync::OnceLock;

use mcr_data::Version;

use crate::dispatch::{
    ConfigClientbound, ConfigServerbound, HandshakeServerbound, LoginClientbound, LoginServerbound,
    PlayClientbound, PlayServerbound, StatusClientbound, StatusServerbound,
};
use crate::{Direction, ProtocolError, ProtocolState};

const NO_VARIANT: u16 = u16::MAX;

/// The packet names of one dispatch enum, in variant order.
pub trait PacketSet {
    const STATE: ProtocolState;
    const DIRECTION: Direction;
    const NAMES: &'static [&'static str];
}

#[derive(Debug, Default)]
struct Table {
    variant_by_id: Box<[u16]>,
    id_by_variant: Box<[i32]>,
    id_by_name: HashMap<&'static str, i32>,
    names: Box<[&'static str]>,
}

/// Packet ids of one protocol version, resolved once from the data crate.
#[derive(Debug)]
pub struct PacketIds {
    tables: [Table; 10],
}

fn slot(state: ProtocolState, direction: Direction) -> usize {
    state as usize * 2 + direction as usize
}

fn short(name: &str) -> &str {
    name.strip_prefix("minecraft:").unwrap_or(name)
}

impl PacketIds {
    pub fn new(version: &Version) -> Result<Self, ProtocolError> {
        let mut ids = Self {
            tables: Default::default(),
        };
        ids.register::<HandshakeServerbound>(version)?;
        ids.register::<StatusClientbound>(version)?;
        ids.register::<StatusServerbound>(version)?;
        ids.register::<LoginClientbound>(version)?;
        ids.register::<LoginServerbound>(version)?;
        ids.register::<ConfigClientbound>(version)?;
        ids.register::<ConfigServerbound>(version)?;
        ids.register::<PlayClientbound>(version)?;
        ids.register::<PlayServerbound>(version)?;
        Ok(ids)
    }

    pub fn v26_3() -> &'static Self {
        static IDS: OnceLock<PacketIds> = OnceLock::new();
        IDS.get_or_init(|| {
            Self::new(mcr_data::v26_3()).expect("dispatch enums match the 26.3 packet report")
        })
    }

    fn register<S: PacketSet>(&mut self, version: &Version) -> Result<(), ProtocolError> {
        let mut id_by_variant = Vec::with_capacity(S::NAMES.len());
        let mut id_by_name = HashMap::with_capacity(S::NAMES.len());
        for &name in S::NAMES {
            let id = version.packet_id(S::STATE, S::DIRECTION, name).ok_or(
                ProtocolError::MissingPacket {
                    packet: name,
                    state: S::STATE,
                    direction: S::DIRECTION,
                },
            )? as i32;
            id_by_variant.push(id);
            id_by_name.insert(short(name), id);
        }
        let len = id_by_variant.iter().copied().max().map_or(0, |id| id + 1);
        let mut variant_by_id = vec![NO_VARIANT; len as usize];
        for (variant, &id) in id_by_variant.iter().enumerate() {
            variant_by_id[id as usize] = variant as u16;
        }
        self.tables[slot(S::STATE, S::DIRECTION)] = Table {
            variant_by_id: variant_by_id.into(),
            id_by_variant: id_by_variant.into(),
            id_by_name,
            names: S::NAMES.iter().map(|&n| short(n)).collect(),
        };
        Ok(())
    }

    pub fn id_by_name(
        &self,
        state: ProtocolState,
        direction: Direction,
        name: &str,
    ) -> Option<i32> {
        let table = &self.tables[slot(state, direction)];
        table.id_by_name.get(short(name)).copied()
    }

    /// The packet's report name without the `minecraft:` prefix.
    pub fn name_by_id(
        &self,
        state: ProtocolState,
        direction: Direction,
        id: i32,
    ) -> Option<&'static str> {
        let variant = self.variant(state, direction, id)?;
        self.tables[slot(state, direction)]
            .names
            .get(variant)
            .copied()
    }

    pub(crate) fn variant(
        &self,
        state: ProtocolState,
        direction: Direction,
        id: i32,
    ) -> Option<usize> {
        let table = &self.tables[slot(state, direction)];
        match table.variant_by_id.get(usize::try_from(id).ok()?) {
            Some(&v) if v != NO_VARIANT => Some(v as usize),
            _ => None,
        }
    }

    pub(crate) fn id(&self, state: ProtocolState, direction: Direction, variant: usize) -> i32 {
        self.tables[slot(state, direction)].id_by_variant[variant]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip_ids() {
        let ids = PacketIds::v26_3();
        let (play, sb) = (ProtocolState::Play, Direction::Serverbound);
        let id = ids.id_by_name(play, sb, "keep_alive").unwrap();
        assert_eq!(ids.name_by_id(play, sb, id), Some("keep_alive"));
        assert_eq!(ids.name_by_id(play, sb, 10_000), None);
        assert_eq!(ids.name_by_id(play, sb, -1), None);
    }
}
