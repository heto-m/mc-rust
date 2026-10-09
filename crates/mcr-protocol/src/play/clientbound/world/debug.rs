use mcr_buf::{BlockPos, Decode, DecodeError, Encode, RestBytes, Result};

use crate::packet;
use crate::types::ChunkPos;

/// Registry order of vanilla's debug subscriptions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub enum DebugSubscription {
    DedicatedServerTickTime,
    Bees,
    Brains,
    Breezes,
    GoalSelectors,
    EntityPaths,
    EntityBlockIntersections,
    BeeHives,
    Pois,
    RedstoneWireOrientations,
    VillageSections,
    Raids,
    Structures,
    GameEventListeners,
    NeighborUpdates,
    GameEvents,
}

/// Values of subscriptions with a trivial codec are decoded; the rest stay as the raw remainder of
/// the packet.
#[derive(Debug, Clone, PartialEq)]
pub enum DebugValue<'a> {
    VillageSections,
    Raids(Vec<BlockPos>),
    NeighborUpdates(BlockPos),
    Undecoded(RestBytes<'a>),
}

impl DebugValue<'_> {
    fn encode_value(&self, out: &mut Vec<u8>) {
        match self {
            Self::VillageSections => {}
            Self::Raids(positions) => positions.encode(out),
            Self::NeighborUpdates(pos) => pos.encode(out),
            Self::Undecoded(rest) => rest.encode(out),
        }
    }
}

impl<'a> DebugValue<'a> {
    fn decode_value(subscription: DebugSubscription, input: &mut &'a [u8]) -> Result<Self> {
        match subscription {
            DebugSubscription::DedicatedServerTickTime => Err(DecodeError::InvalidValue {
                what: "debug subscription without a value codec",
                value: subscription as i64,
            }),
            DebugSubscription::VillageSections => Ok(Self::VillageSections),
            DebugSubscription::Raids => Vec::decode(input).map(Self::Raids),
            DebugSubscription::NeighborUpdates => {
                BlockPos::decode(input).map(Self::NeighborUpdates)
            }
            _ => RestBytes::decode(input).map(Self::Undecoded),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DebugUpdate<'a> {
    pub subscription: DebugSubscription,
    pub value: Option<DebugValue<'a>>,
}

impl Encode for DebugUpdate<'_> {
    fn encode(&self, out: &mut Vec<u8>) {
        self.subscription.encode(out);
        self.value.is_some().encode(out);
        if let Some(value) = &self.value {
            value.encode_value(out);
        }
    }
}

impl<'a> Decode<'a> for DebugUpdate<'a> {
    fn decode(input: &mut &'a [u8]) -> Result<Self> {
        let subscription = DebugSubscription::decode(input)?;
        let value = if bool::decode(input)? {
            Some(DebugValue::decode_value(subscription, input)?)
        } else {
            None
        };
        Ok(Self {
            subscription,
            value,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DebugEventData<'a> {
    pub subscription: DebugSubscription,
    pub value: DebugValue<'a>,
}

impl Encode for DebugEventData<'_> {
    fn encode(&self, out: &mut Vec<u8>) {
        self.subscription.encode(out);
        self.value.encode_value(out);
    }
}

impl<'a> Decode<'a> for DebugEventData<'a> {
    fn decode(input: &mut &'a [u8]) -> Result<Self> {
        let subscription = DebugSubscription::decode(input)?;
        Ok(Self {
            subscription,
            value: DebugValue::decode_value(subscription, input)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct DebugBlockValue<'a> {
    pub pos: BlockPos,
    pub update: DebugUpdate<'a>,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct DebugChunkValue<'a> {
    pub pos: ChunkPos,
    pub update: DebugUpdate<'a>,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct DebugEntityValue<'a> {
    #[mcr(varint)]
    pub entity_id: i32,
    pub update: DebugUpdate<'a>,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct DebugEvent<'a> {
    pub event: DebugEventData<'a>,
}

packet!(DebugBlockValue<'a>, "debug/block_value", Clientbound);
packet!(DebugChunkValue<'a>, "debug/chunk_value", Clientbound);
packet!(DebugEntityValue<'a>, "debug/entity_value", Clientbound);
packet!(DebugEvent<'a>, "debug/event", Clientbound);

#[cfg(test)]
mod tests {
    use mcr_buf::{from_bytes_exact, to_bytes};

    use super::*;

    #[test]
    fn neighbor_update_event_bytes() {
        let event = DebugEvent {
            event: DebugEventData {
                subscription: DebugSubscription::NeighborUpdates,
                value: DebugValue::NeighborUpdates(BlockPos::new(1, 0, 0)),
            },
        };
        let bytes = to_bytes(&event);
        assert_eq!(bytes, [14, 0, 0, 0, 0x40, 0, 0, 0, 0]);
        assert_eq!(from_bytes_exact::<DebugEvent>(&bytes).unwrap(), event);
    }

    #[test]
    fn update_with_and_without_value() {
        let absent = DebugEntityValue {
            entity_id: 3,
            update: DebugUpdate {
                subscription: DebugSubscription::Raids,
                value: None,
            },
        };
        assert_eq!(to_bytes(&absent), [3, 11, 0]);
        let present = DebugBlockValue {
            pos: BlockPos::new(0, 5, 0),
            update: DebugUpdate {
                subscription: DebugSubscription::Raids,
                value: Some(DebugValue::Raids(vec![BlockPos::new(0, 1, 0)])),
            },
        };
        let bytes = to_bytes(&present);
        assert_eq!(&bytes[8..], [11, 1, 1, 0, 0, 0, 0, 0, 0, 0, 1]);
        assert_eq!(
            from_bytes_exact::<DebugBlockValue>(&bytes).unwrap(),
            present
        );
    }

    #[test]
    fn complex_kinds_stay_undecoded() {
        let bytes = [1, 1, 9, 9, 9];
        let full = [&[7][..], &bytes].concat();
        let update = from_bytes_exact::<DebugEntityValue>(&full).unwrap();
        assert_eq!(
            update.update.value,
            Some(DebugValue::Undecoded(RestBytes(&[9, 9, 9])))
        );
        assert_eq!(to_bytes(&update), full);
    }

    #[test]
    fn rejects_unknown_and_valueless_subscriptions() {
        assert!(matches!(
            from_bytes_exact::<DebugEvent>(&[99]),
            Err(DecodeError::InvalidValue { .. })
        ));
        assert!(from_bytes_exact::<DebugEvent>(&[0]).is_err());
    }
}
