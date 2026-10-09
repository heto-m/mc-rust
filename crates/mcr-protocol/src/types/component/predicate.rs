use mcr_buf::{Decode, Encode, LengthPrefixed, Result, VarInt};
use mcr_nbt::{NetworkCompound, NetworkNbt};

use super::DataComponent;
use super::common::{HolderSet, either_codec};

#[derive(Debug, Clone, PartialEq, Default, Encode, Decode)]
pub struct AdventureModePredicate(pub Vec<BlockPredicate>);

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct BlockPredicate {
    pub blocks: Option<HolderSet>,
    pub properties: Option<StatePropertiesPredicate>,
    pub nbt: Option<NetworkCompound>,
    pub components: DataComponentMatchers,
}

#[derive(Debug, Clone, PartialEq, Default, Encode, Decode)]
pub struct StatePropertiesPredicate(pub Vec<PropertyMatcher>);

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct PropertyMatcher {
    pub name: String,
    pub matcher: ValueMatcher,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RangedMatcher {
    pub min: Option<String>,
    pub max: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueMatcher {
    Exact(String),
    Ranged(RangedMatcher),
}
either_codec!(ValueMatcher, Exact(String), Ranged(RangedMatcher));

#[derive(Debug, Clone, PartialEq, Default, Encode, Decode)]
pub struct DataComponentMatchers {
    pub exact: Vec<DataComponent>,
    pub partial: LengthPrefixed<ComponentPredicate, 64>,
}

/// Either a dedicated predicate type or an "any value" check on a component type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PredicateType {
    Predicate(i32),
    AnyValue(i32),
}

impl Encode for PredicateType {
    fn encode(&self, out: &mut Vec<u8>) {
        match *self {
            Self::Predicate(id) => {
                true.encode(out);
                VarInt(id).encode(out);
            }
            Self::AnyValue(id) => {
                false.encode(out);
                VarInt(id).encode(out);
            }
        }
    }
}

impl Decode<'_> for PredicateType {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        let predicate = bool::decode(input)?;
        let id = VarInt::decode(input)?.0;
        Ok(if predicate {
            Self::Predicate(id)
        } else {
            Self::AnyValue(id)
        })
    }
}

/// The payload is the predicate's NBT form, kept verbatim.
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct ComponentPredicate {
    pub kind: PredicateType,
    pub value: NetworkNbt,
}
