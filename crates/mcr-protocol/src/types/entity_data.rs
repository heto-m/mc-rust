use mcr_buf::{
    BlockPos, BoundedString, Decode, DecodeError, Encode, IdOr, Identifier, LengthPrefixed, Result,
    read_varint, write_varint,
};
use uuid::Uuid;

use super::{Component, ItemStack, Particle, Vec3f};

pub const END_MARKER: u8 = 0xFF;

macro_rules! id_enum {
    ($($name:ident { $($variant:ident = $id:literal),+ $(,)? })+) => {$(
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            pub const fn id(self) -> i32 {
                match self {
                    $(Self::$variant => $id),+
                }
            }

            pub const fn from_id(id: i32) -> Option<Self> {
                match id {
                    $($id => Some(Self::$variant),)+
                    _ => None,
                }
            }
        }

        impl Encode for $name {
            fn encode(&self, out: &mut Vec<u8>) {
                write_varint(out, self.id());
            }
        }

        impl Decode<'_> for $name {
            fn decode(input: &mut &[u8]) -> Result<Self> {
                let id = read_varint(input)?;
                Self::from_id(id).ok_or(DecodeError::InvalidValue {
                    what: stringify!($name),
                    value: id.into(),
                })
            }
        }
    )+};
}

id_enum! {
    Pose {
        Standing = 0,
        FallFlying = 1,
        Sleeping = 2,
        Swimming = 3,
        SpinAttack = 4,
        Crouching = 5,
        LongJumping = 6,
        Dying = 7,
        Croaking = 8,
        UsingTongue = 9,
        Sitting = 10,
        Roaring = 11,
        Sniffing = 12,
        Emerging = 13,
        Digging = 14,
        Sliding = 15,
        Shooting = 16,
        Inhaling = 17,
    }
    Direction {
        Down = 0,
        Up = 1,
        North = 2,
        South = 3,
        West = 4,
        East = 5,
    }
    HumanoidArm {
        Left = 0,
        Right = 1,
    }
    DyeColor {
        White = 0,
        Orange = 1,
        Magenta = 2,
        LightBlue = 3,
        Yellow = 4,
        Lime = 5,
        Pink = 6,
        Gray = 7,
        LightGray = 8,
        Cyan = 9,
        Purple = 10,
        Blue = 11,
        Brown = 12,
        Green = 13,
        Red = 14,
        Black = 15,
    }
    ArmadilloState {
        Idle = 0,
        Rolling = 1,
        Scared = 2,
        Unrolling = 3,
    }
    SnifferState {
        Idling = 0,
        FeelingHappy = 1,
        Scenting = 2,
        Sniffing = 3,
        Searching = 4,
        Digging = 5,
        Rising = 6,
    }
    WeatherState {
        Unaffected = 0,
        Exposed = 1,
        Weathered = 2,
        Oxidized = 3,
    }
    CopperGolemState {
        Idle = 0,
        GettingItem = 1,
        GettingNoItem = 2,
        DroppingItem = 3,
        DroppingNoItem = 4,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Encode, Decode)]
pub struct Rotations {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct GlobalPos {
    pub dimension: Identifier,
    pub pos: BlockPos,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct VillagerData {
    #[mcr(varint)]
    pub kind: i32,
    #[mcr(varint)]
    pub profession: i32,
    #[mcr(varint)]
    pub level: i32,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct PaintingVariantData {
    #[mcr(varint)]
    pub width: i32,
    #[mcr(varint)]
    pub height: i32,
    pub asset_id: Identifier,
    pub title: Option<Component>,
    pub author: Option<Component>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ProfileProperty {
    pub name: BoundedString<64>,
    pub value: BoundedString<32767>,
    pub signature: Option<BoundedString<1024>>,
}

pub type ProfileProperties = LengthPrefixed<ProfileProperty, 16>;

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct GameProfile {
    pub id: Uuid,
    pub name: BoundedString<16>,
    pub properties: ProfileProperties,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Encode, Decode)]
pub struct PartialProfile {
    pub name: Option<BoundedString<16>>,
    pub id: Option<Uuid>,
    pub properties: ProfileProperties,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[mcr(tag = u8)]
#[repr(u8)]
pub enum ProfileSource {
    Partial(PartialProfile) = 0,
    Full(GameProfile) = 1,
}

/// `slim` is the player model type: `true` for slim arms, `false` for wide.
#[derive(Debug, Clone, PartialEq, Eq, Default, Encode, Decode)]
pub struct SkinPatch {
    pub body: Option<Identifier>,
    pub cape: Option<Identifier>,
    pub elytra: Option<Identifier>,
    pub slim: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ResolvableProfile {
    pub source: ProfileSource,
    pub skin_patch: SkinPatch,
}

/// Empty is 0 and a value `v` is `v + 1`.
mod optional_uint {
    use super::*;

    pub fn encode(value: &Option<u32>, out: &mut Vec<u8>) {
        write_varint(out, value.map_or(0, |v| v.wrapping_add(1) as i32));
    }

    pub fn decode(input: &mut &[u8]) -> Result<Option<u32>> {
        Ok(match read_varint(input)? as u32 {
            0 => None,
            v => Some(v - 1),
        })
    }
}

/// Empty is state id 0 (air is never stored).
mod optional_block_state {
    use super::*;

    pub fn encode(value: &Option<i32>, out: &mut Vec<u8>) {
        write_varint(out, value.unwrap_or(0));
    }

    pub fn decode(input: &mut &[u8]) -> Result<Option<i32>> {
        Ok(match read_varint(input)? {
            0 => None,
            v => Some(v),
        })
    }
}

/// One variant per serializer; the discriminant is the serializer id (26.3 registration order).
/// Registry-backed values (variants, block states) are raw registry ids.
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
#[allow(clippy::large_enum_variant)]
#[repr(u8)]
pub enum EntityDataValue {
    Byte(i8) = 0,
    Int(#[mcr(varint)] i32) = 1,
    Long(#[mcr(varlong)] i64) = 2,
    Float(f32) = 3,
    String(String) = 4,
    Component(Component) = 5,
    OptionalComponent(Option<Component>) = 6,
    ItemStack(ItemStack) = 7,
    Boolean(bool) = 8,
    Rotations(Rotations) = 9,
    BlockPos(BlockPos) = 10,
    OptionalBlockPos(Option<BlockPos>) = 11,
    Direction(Direction) = 12,
    OptionalLivingEntityRef(Option<Uuid>) = 13,
    BlockState(#[mcr(varint)] i32) = 14,
    OptionalBlockState(#[mcr(with = optional_block_state)] Option<i32>) = 15,
    Particle(Particle) = 16,
    Particles(Vec<Particle>) = 17,
    VillagerData(VillagerData) = 18,
    OptionalUnsignedInt(#[mcr(with = optional_uint)] Option<u32>) = 19,
    Pose(Pose) = 20,
    CatVariant(#[mcr(varint)] i32) = 21,
    CatSoundVariant(#[mcr(varint)] i32) = 22,
    CowVariant(#[mcr(varint)] i32) = 23,
    CowSoundVariant(#[mcr(varint)] i32) = 24,
    WolfVariant(#[mcr(varint)] i32) = 25,
    WolfSoundVariant(#[mcr(varint)] i32) = 26,
    FrogVariant(#[mcr(varint)] i32) = 27,
    PigVariant(#[mcr(varint)] i32) = 28,
    PigSoundVariant(#[mcr(varint)] i32) = 29,
    ChickenVariant(#[mcr(varint)] i32) = 30,
    ChickenSoundVariant(#[mcr(varint)] i32) = 31,
    ZombieNautilusVariant(#[mcr(varint)] i32) = 32,
    OptionalGlobalPos(Option<GlobalPos>) = 33,
    PaintingVariant(IdOr<PaintingVariantData>) = 34,
    SnifferState(SnifferState) = 35,
    ArmadilloState(ArmadilloState) = 36,
    CopperGolemState(CopperGolemState) = 37,
    WeatheringCopperState(WeatherState) = 38,
    Vector3(Vec3f) = 39,
    Quaternion([f32; 4]) = 40,
    ResolvableProfile(Box<ResolvableProfile>) = 41,
    HumanoidArm(HumanoidArm) = 42,
    DyeColor(DyeColor) = 43,
}

pub const SERIALIZER_COUNT: i32 = 44;

#[derive(Debug, Clone, PartialEq)]
pub struct EntityDataEntry {
    pub index: u8,
    pub value: EntityDataValue,
}

/// Entries until an index byte of 0xFF.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EntityDataValues {
    pub entries: Vec<EntityDataEntry>,
}

impl Encode for EntityDataValues {
    fn encode(&self, out: &mut Vec<u8>) {
        for entry in &self.entries {
            entry.index.encode(out);
            entry.value.encode(out);
        }
        out.push(END_MARKER);
    }
}

impl Decode<'_> for EntityDataValues {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        let mut entries = Vec::new();
        loop {
            let index = u8::decode(input)?;
            if index == END_MARKER {
                return Ok(Self { entries });
            }
            entries.push(EntityDataEntry {
                index,
                value: EntityDataValue::decode(input)?,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use mcr_buf::{DecodeError, from_bytes_exact, to_bytes};

    use super::*;
    use crate::test_util::roundtrip;

    fn profile() -> ResolvableProfile {
        ResolvableProfile {
            source: ProfileSource::Full(GameProfile {
                id: Uuid::from_u128(7),
                name: BoundedString("Steve".into()),
                properties: LengthPrefixed(vec![ProfileProperty {
                    name: BoundedString("textures".into()),
                    value: BoundedString("v".into()),
                    signature: Some(BoundedString("s".into())),
                }]),
            }),
            skin_patch: SkinPatch {
                body: Some(Identifier::parse("a:b").unwrap()),
                slim: Some(true),
                ..Default::default()
            },
        }
    }

    fn samples() -> Vec<EntityDataValue> {
        use EntityDataValue as V;
        vec![
            V::Byte(-3),
            V::Int(300),
            V::Long(-1),
            V::Float(1.5),
            V::String("hi".into()),
            V::Boolean(true),
            V::Rotations(Rotations {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            }),
            V::BlockPos(BlockPos::new(1, 2, 3)),
            V::OptionalBlockPos(None),
            V::OptionalBlockPos(Some(BlockPos::new(-1, 64, 9))),
            V::Direction(Direction::East),
            V::OptionalLivingEntityRef(None),
            V::OptionalLivingEntityRef(Some(Uuid::from_u128(42))),
            V::BlockState(1234),
            V::OptionalBlockState(None),
            V::OptionalBlockState(Some(9)),
            V::Particle(Particle::Simple(11)),
            V::Particles(vec![
                Particle::Simple(11),
                Particle::DragonBreath { power: 1.0 },
            ]),
            V::VillagerData(VillagerData {
                kind: 2,
                profession: 5,
                level: 3,
            }),
            V::OptionalUnsignedInt(None),
            V::OptionalUnsignedInt(Some(0)),
            V::OptionalUnsignedInt(Some(77)),
            V::Pose(Pose::Sleeping),
            V::CatVariant(1),
            V::CatSoundVariant(2),
            V::CowVariant(3),
            V::CowSoundVariant(4),
            V::WolfVariant(5),
            V::WolfSoundVariant(6),
            V::FrogVariant(7),
            V::PigVariant(8),
            V::PigSoundVariant(9),
            V::ChickenVariant(10),
            V::ChickenSoundVariant(11),
            V::ZombieNautilusVariant(12),
            V::OptionalGlobalPos(None),
            V::OptionalGlobalPos(Some(GlobalPos {
                dimension: Identifier::parse("overworld").unwrap(),
                pos: BlockPos::new(5, 6, 7),
            })),
            V::PaintingVariant(IdOr::Id(3)),
            V::PaintingVariant(IdOr::Inline(PaintingVariantData {
                width: 2,
                height: 1,
                asset_id: Identifier::parse("x:y").unwrap(),
                title: None,
                author: None,
            })),
            V::SnifferState(SnifferState::Digging),
            V::ArmadilloState(ArmadilloState::Scared),
            V::CopperGolemState(CopperGolemState::DroppingNoItem),
            V::WeatheringCopperState(WeatherState::Oxidized),
            V::Vector3(Vec3f {
                x: 1.0,
                y: 0.0,
                z: -1.0,
            }),
            V::Quaternion([0.0, 0.0, 0.0, 1.0]),
            V::ResolvableProfile(Box::new(profile())),
            V::ResolvableProfile(Box::new(ResolvableProfile {
                source: ProfileSource::Partial(PartialProfile {
                    name: Some(BoundedString("Alex".into())),
                    ..Default::default()
                }),
                skin_patch: SkinPatch::default(),
            })),
            V::HumanoidArm(HumanoidArm::Right),
            V::DyeColor(DyeColor::Black),
        ]
    }

    #[test]
    fn roundtrip_every_serializer() {
        let mut seen = [false; SERIALIZER_COUNT as usize];
        for value in samples() {
            let bytes = roundtrip(&value);
            seen[bytes[0] as usize] = true;
        }
        let missing: Vec<_> = (0..seen.len())
            .filter(|&i| !seen[i] && ![5, 6, 7].contains(&i))
            .collect();
        assert!(missing.is_empty(), "untested serializer ids {missing:?}");
    }

    #[test]
    #[ignore = "needs Component"]
    fn component_serializers() {
        let values = [
            EntityDataValue::Component(Component::default()),
            EntityDataValue::OptionalComponent(None),
            EntityDataValue::OptionalComponent(Some(Component::default())),
        ];
        for v in values {
            roundtrip(&v);
        }
    }

    #[test]
    #[ignore = "needs ItemStack"]
    fn item_stack_serializer() {
        roundtrip(&EntityDataValue::ItemStack(ItemStack::default()));
    }

    #[test]
    fn serializer_ids_follow_registration_order() {
        use EntityDataValue as V;
        let id = |v: V| to_bytes(&v)[0];
        assert_eq!(id(V::Byte(0)), 0);
        assert_eq!(id(V::Boolean(false)), 8);
        assert_eq!(id(V::Particle(Particle::Simple(0))), 16);
        assert_eq!(id(V::OptionalUnsignedInt(None)), 19);
        assert_eq!(id(V::Pose(Pose::Standing)), 20);
        assert_eq!(id(V::ZombieNautilusVariant(0)), 32);
        assert_eq!(id(V::OptionalGlobalPos(None)), 33);
        assert_eq!(id(V::PaintingVariant(IdOr::Id(0))), 34);
        assert_eq!(id(V::SnifferState(SnifferState::Idling)), 35);
        assert_eq!(id(V::WeatheringCopperState(WeatherState::Unaffected)), 38);
        assert_eq!(id(V::DyeColor(DyeColor::White)), 43);
    }

    #[test]
    fn typical_list_bytes() {
        let list = EntityDataValues {
            entries: vec![
                EntityDataEntry {
                    index: 0,
                    value: EntityDataValue::Byte(0x02),
                },
                EntityDataEntry {
                    index: 1,
                    value: EntityDataValue::Int(300),
                },
                EntityDataEntry {
                    index: 6,
                    value: EntityDataValue::Pose(Pose::Crouching),
                },
            ],
        };
        assert_eq!(
            roundtrip(&list),
            [0, 0, 0x02, 1, 1, 0xAC, 0x02, 6, 20, 5, 0xFF]
        );
    }

    #[test]
    fn empty_list_is_marker_only() {
        assert_eq!(roundtrip(&EntityDataValues::default()), [0xFF]);
    }

    #[test]
    fn optional_encodings() {
        use EntityDataValue as V;
        assert_eq!(to_bytes(&V::OptionalUnsignedInt(None)), [19, 0]);
        assert_eq!(to_bytes(&V::OptionalUnsignedInt(Some(4))), [19, 5]);
        assert_eq!(to_bytes(&V::OptionalBlockState(None)), [15, 0]);
        assert_eq!(to_bytes(&V::OptionalBlockPos(None)), [11, 0]);
    }

    #[test]
    fn painting_holder_bytes() {
        let bytes = to_bytes(&EntityDataValue::PaintingVariant(IdOr::Id(2)));
        assert_eq!(bytes, [34, 3]);
    }

    #[test]
    fn rejects_unknown_serializer() {
        let err = from_bytes_exact::<EntityDataValues>(&[0, 44, 0, 0xFF]).unwrap_err();
        assert!(matches!(err, DecodeError::InvalidValue { .. }));
    }

    #[test]
    fn rejects_truncated_and_bad_enum() {
        assert!(from_bytes_exact::<EntityDataValues>(&[0, 0, 1]).is_err());
        assert!(from_bytes_exact::<EntityDataValues>(&[6, 20, 18, 0xFF]).is_err());
    }
}
