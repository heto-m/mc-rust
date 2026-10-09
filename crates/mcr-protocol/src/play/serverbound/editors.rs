use mcr_buf::{
    BlockPos, BoundedString, Decode, Encode, Identifier, Result, VarInt, decode_str, encode_str,
    read_varlong, write_varlong,
};

use super::shared::{FlagByte, Rotation, Vec3i};
use crate::packet;
use crate::types::{Component, game::SignSide};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub enum CommandBlockMode {
    Sequence,
    Auto,
    Redstone,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SetCommandBlock {
    pub pos: BlockPos,
    pub command: String,
    pub mode: CommandBlockMode,
    pub track_output: bool,
    pub conditional: bool,
    pub automatic: bool,
}

impl Encode for SetCommandBlock {
    fn encode(&self, out: &mut Vec<u8>) {
        self.pos.encode(out);
        self.command.encode(out);
        self.mode.encode(out);
        FlagByte(0)
            .with(0, self.track_output)
            .with(1, self.conditional)
            .with(2, self.automatic)
            .encode(out);
    }
}

impl Decode<'_> for SetCommandBlock {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        let pos = BlockPos::decode(input)?;
        let command = String::decode(input)?;
        let mode = CommandBlockMode::decode(input)?;
        let flags = FlagByte::decode(input)?;
        Ok(Self {
            pos,
            command,
            mode,
            track_output: flags.bit(0),
            conditional: flags.bit(1),
            automatic: flags.bit(2),
        })
    }
}

packet!(SetCommandBlock, "set_command_block", Serverbound);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Encode, Decode)]
pub struct SetCommandMinecart {
    #[mcr(varint)]
    pub entity_id: i32,
    pub command: String,
    pub track_output: bool,
}

packet!(SetCommandMinecart, "set_command_minecart", Serverbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JointType {
    Rollable,
    Aligned,
}

mod joint_name {
    use super::*;

    pub fn encode(value: &JointType, out: &mut Vec<u8>) {
        encode_str(
            match value {
                JointType::Rollable => "rollable",
                JointType::Aligned => "aligned",
            },
            out,
        );
    }

    pub fn decode(input: &mut &[u8]) -> Result<JointType> {
        Ok(match decode_str(input, mcr_buf::MAX_STRING_CHARS)? {
            "rollable" => JointType::Rollable,
            _ => JointType::Aligned,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Encode, Decode)]
pub struct SetJigsawBlock {
    pub pos: BlockPos,
    pub name: Identifier,
    pub target: Identifier,
    pub pool: Identifier,
    pub final_state: String,
    #[mcr(with = joint_name)]
    pub joint: JointType,
    #[mcr(varint)]
    pub selection_priority: i32,
    #[mcr(varint)]
    pub placement_priority: i32,
}

packet!(SetJigsawBlock, "set_jigsaw_block", Serverbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub struct JigsawGenerate {
    pub pos: BlockPos,
    #[mcr(varint)]
    pub levels: i32,
    pub keep_jigsaws: bool,
}

packet!(JigsawGenerate, "jigsaw_generate", Serverbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub enum StructureUpdateType {
    UpdateData,
    SaveArea,
    LoadArea,
    ScanArea,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub enum StructureMode {
    Save,
    Load,
    Corner,
    Data,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub enum Mirror {
    None,
    LeftRight,
    FrontBack,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SetStructureBlock {
    pub pos: BlockPos,
    pub update_type: StructureUpdateType,
    pub mode: StructureMode,
    pub name: String,
    pub offset: [i8; 3],
    pub size: [i8; 3],
    pub mirror: Mirror,
    pub rotation: Rotation,
    pub data: String,
    pub ignore_entities: bool,
    pub strict: bool,
    pub show_air: bool,
    pub show_bounding_box: bool,
    pub integrity: f32,
    pub seed: i64,
}

impl SetStructureBlock {
    pub const MAX_EXTENT: i8 = 48;
}

impl Encode for SetStructureBlock {
    fn encode(&self, out: &mut Vec<u8>) {
        self.pos.encode(out);
        self.update_type.encode(out);
        self.mode.encode(out);
        self.name.encode(out);
        self.offset.encode(out);
        self.size.encode(out);
        self.mirror.encode(out);
        self.rotation.encode(out);
        self.data.encode(out);
        self.integrity.encode(out);
        write_varlong(out, self.seed);
        FlagByte(0)
            .with(0, self.ignore_entities)
            .with(1, self.show_air)
            .with(2, self.show_bounding_box)
            .with(3, self.strict)
            .encode(out);
    }
}

impl Decode<'_> for SetStructureBlock {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        let max = Self::MAX_EXTENT;
        let pos = BlockPos::decode(input)?;
        let update_type = StructureUpdateType::decode(input)?;
        let mode = StructureMode::decode(input)?;
        let name = String::decode(input)?;
        let offset = <[i8; 3]>::decode(input)?.map(|v| v.clamp(-max, max));
        let size = <[i8; 3]>::decode(input)?.map(|v| v.clamp(0, max));
        let mirror = Mirror::decode(input)?;
        let rotation = Rotation::decode(input)?;
        let data = BoundedString::<128>::decode(input)?.0;
        let integrity = f32::decode(input)?.clamp(0.0, 1.0);
        let seed = read_varlong(input)?;
        let flags = FlagByte::decode(input)?;
        Ok(Self {
            pos,
            update_type,
            mode,
            name,
            offset,
            size,
            mirror,
            rotation,
            data,
            ignore_entities: flags.bit(0),
            show_air: flags.bit(1),
            show_bounding_box: flags.bit(2),
            strict: flags.bit(3),
            integrity,
            seed,
        })
    }
}

packet!(SetStructureBlock, "set_structure_block", Serverbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub enum TestBlockMode {
    Start,
    Log,
    Fail,
    Accept,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Encode, Decode)]
pub struct SetTestBlock {
    pub pos: BlockPos,
    pub mode: TestBlockMode,
    pub message: String,
}

packet!(SetTestBlock, "set_test_block", Serverbound);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Encode, Decode)]
pub struct SignUpdate {
    pub pos: BlockPos,
    pub lines: [BoundedString<384>; 4],
    pub side: SignSide,
}

packet!(SignUpdate, "sign_update", Serverbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Encode, Decode)]
pub struct SetBeacon {
    pub primary: Option<VarInt>,
    pub secondary: Option<VarInt>,
}

packet!(SetBeacon, "set_beacon", Serverbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub enum TestInstanceAction {
    Init,
    Query,
    Set,
    Reset,
    Save,
    Export,
    Run,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub enum TestInstanceStatus {
    Cleared,
    Running,
    Finished,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct TestInstanceData {
    pub test: Option<Identifier>,
    pub size: Vec3i,
    pub rotation: Rotation,
    pub ignore_entities: bool,
    pub status: TestInstanceStatus,
    pub error_message: Option<Component>,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct TestInstanceBlockAction {
    pub pos: BlockPos,
    pub action: TestInstanceAction,
    pub data: TestInstanceData,
}

packet!(
    TestInstanceBlockAction,
    "test_instance_block_action",
    Serverbound
);

#[cfg(test)]
mod tests {
    use mcr_buf::{DecodeError, from_bytes_exact, to_bytes};

    use super::*;
    use crate::play::serverbound::shared::roundtrips;
    use crate::test_util::roundtrip;

    fn line(s: &str) -> BoundedString<384> {
        BoundedString(s.into())
    }

    fn sample_structure() -> SetStructureBlock {
        SetStructureBlock {
            pos: BlockPos::new(1, 2, 3),
            update_type: StructureUpdateType::SaveArea,
            mode: StructureMode::Save,
            name: "base".into(),
            offset: [-1, 0, 2],
            size: [4, 5, 6],
            mirror: Mirror::LeftRight,
            rotation: Rotation::Clockwise90,
            data: "d".into(),
            ignore_entities: true,
            strict: true,
            show_air: false,
            show_bounding_box: true,
            integrity: 0.5,
            seed: 123_456_789_012,
        }
    }

    roundtrips! {
        command_block: SetCommandBlock {
            pos: BlockPos::new(1, 2, 3),
            command: "say hi".into(),
            mode: CommandBlockMode::Auto,
            track_output: true,
            conditional: false,
            automatic: true,
        },
        minecart: SetCommandMinecart { entity_id: 12, command: "x".into(), track_output: false },
        jigsaw: SetJigsawBlock {
            pos: BlockPos::new(0, 0, 0),
            name: Identifier::parse("a").unwrap(),
            target: Identifier::parse("b").unwrap(),
            pool: Identifier::parse("c:d").unwrap(),
            final_state: "minecraft:air".into(),
            joint: JointType::Rollable,
            selection_priority: 1,
            placement_priority: 300,
        },
        jigsaw_generate: JigsawGenerate { pos: BlockPos::new(1, 1, 1), levels: 7, keep_jigsaws: true },
        structure: sample_structure(),
        test_block: SetTestBlock { pos: BlockPos::new(1, 1, 1), mode: TestBlockMode::Fail, message: "m".into() },
        sign: SignUpdate { pos: BlockPos::new(1, 1, 1), lines: [line("a"), line(""), line("c"), line("")], side: SignSide::Front },
        beacon: SetBeacon { primary: Some(VarInt(1)), secondary: None },
    }

    #[test]
    fn test_instance_roundtrip() {
        roundtrip(&TestInstanceBlockAction {
            pos: BlockPos::new(1, 2, 3),
            action: TestInstanceAction::Run,
            data: TestInstanceData {
                test: Some(Identifier::parse("t").unwrap()),
                size: Vec3i { x: 1, y: 2, z: 3 },
                rotation: Rotation::Clockwise180,
                ignore_entities: true,
                status: TestInstanceStatus::Finished,
                error_message: Some(Component::default()),
            },
        });
    }

    #[test]
    fn command_block_flags() {
        let bytes = to_bytes(&SetCommandBlock {
            pos: BlockPos::new(0, 0, 0),
            command: String::new(),
            mode: CommandBlockMode::Redstone,
            track_output: true,
            conditional: false,
            automatic: true,
        });
        assert_eq!(bytes, [0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 5]);
    }

    #[test]
    fn jigsaw_joint_is_a_string() {
        let mut out = Vec::new();
        joint_name::encode(&JointType::Aligned, &mut out);
        assert_eq!(out, [7, b'a', b'l', b'i', b'g', b'n', b'e', b'd']);
        assert_eq!(
            joint_name::decode(&mut &[3, b'x', b'y', b'z'][..]).unwrap(),
            JointType::Aligned
        );
    }

    #[test]
    fn structure_flag_layout_and_clamping() {
        let mut packet = sample_structure();
        let bytes = to_bytes(&packet);
        assert_eq!(*bytes.last().unwrap(), 1 | 4 | 8);

        packet.offset = [100, -100, 0];
        packet.size = [-3, 99, 1];
        let clamped = from_bytes_exact::<SetStructureBlock>(&to_bytes(&packet)).unwrap();
        assert_eq!(clamped.offset, [48, -48, 0]);
        assert_eq!(clamped.size, [0, 48, 1]);
    }

    #[test]
    fn sign_has_no_line_count() {
        let packet = SignUpdate {
            pos: BlockPos::new(0, 5, 0),
            lines: [line("a"), line(""), line("c"), line("")],
            side: SignSide::Front,
        };
        assert_eq!(
            roundtrip(&packet),
            [0, 0, 0, 0, 0, 0, 0, 5, 1, b'a', 0, 1, b'c', 0, 1]
        );
    }

    #[test]
    fn structure_bytes() {
        let packet = SetStructureBlock {
            pos: BlockPos::new(0, 5, 0),
            seed: 300,
            name: "n".into(),
            ..sample_structure()
        };
        let expected = [
            &[0, 0, 0, 0, 0, 0, 0, 5][..],
            &[1, 0, 1, b'n'],
            &[0xff, 0, 2, 4, 5, 6],
            &[1, 1, 1, b'd'],
            &[0x3f, 0, 0, 0],
            &[0xac, 0x02, 1 | 4 | 8],
        ]
        .concat();
        assert_eq!(roundtrip(&packet), expected);
    }

    #[test]
    fn jigsaw_bytes() {
        let packet = SetJigsawBlock {
            pos: BlockPos::new(0, 0, 0),
            name: Identifier::parse("a:b").unwrap(),
            target: Identifier::parse("c:d").unwrap(),
            pool: Identifier::parse("e:f").unwrap(),
            final_state: "s".into(),
            joint: JointType::Rollable,
            selection_priority: 1,
            placement_priority: 300,
        };
        let expected = [
            &[0; 8][..],
            &[
                3, b'a', b':', b'b', 3, b'c', b':', b'd', 3, b'e', b':', b'f',
            ],
            &[1, b's', 8],
            b"rollable",
            &[1, 0xac, 0x02],
        ]
        .concat();
        assert_eq!(roundtrip(&packet), expected);
        assert_eq!(
            roundtrip(&JigsawGenerate {
                pos: BlockPos::new(0, 0, 0),
                levels: 7,
                keep_jigsaws: true
            }),
            [0, 0, 0, 0, 0, 0, 0, 0, 7, 1]
        );
    }

    #[test]
    fn small_editor_bytes() {
        assert_eq!(
            roundtrip(&SetCommandMinecart {
                entity_id: 300,
                command: "x".into(),
                track_output: true
            }),
            [0xac, 0x02, 1, b'x', 1]
        );
        assert_eq!(
            roundtrip(&SetTestBlock {
                pos: BlockPos::new(0, 0, 0),
                mode: TestBlockMode::Fail,
                message: "m".into()
            }),
            [0, 0, 0, 0, 0, 0, 0, 0, 2, 1, b'm']
        );
        assert_eq!(
            roundtrip(&SetBeacon {
                primary: Some(VarInt(1)),
                secondary: None
            }),
            [1, 1, 0]
        );
    }

    #[test]
    fn test_instance_bytes() {
        let packet = TestInstanceBlockAction {
            pos: BlockPos::new(0, 0, 0),
            action: TestInstanceAction::Run,
            data: TestInstanceData {
                test: Some(Identifier::parse("a:b").unwrap()),
                size: Vec3i { x: 1, y: 2, z: 300 },
                rotation: Rotation::Clockwise180,
                ignore_entities: true,
                status: TestInstanceStatus::Finished,
                error_message: None,
            },
        };
        assert_eq!(
            roundtrip(&packet),
            [
                0, 0, 0, 0, 0, 0, 0, 0, 6, 1, 3, b'a', b':', b'b', 1, 2, 0xac, 0x02, 2, 1, 2, 0
            ]
        );
    }

    #[test]
    fn invalid_facing_like_enums_rejected() {
        assert!(matches!(
            from_bytes_exact::<TestBlockMode>(&[9]),
            Err(DecodeError::InvalidValue { .. })
        ));
    }
}
