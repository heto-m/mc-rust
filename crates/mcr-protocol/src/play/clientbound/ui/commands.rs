use mcr_buf::{
    Decode, DecodeError, Encode, Identifier, Result, read_len, read_varint, take, write_varint,
};

use crate::packet;

const TYPE_MASK: u8 = 3;
const TYPE_ROOT: u8 = 0;
const TYPE_LITERAL: u8 = 1;
const TYPE_ARGUMENT: u8 = 2;
const FLAG_EXECUTABLE: u8 = 4;
const FLAG_REDIRECT: u8 = 8;
const FLAG_SUGGESTIONS: u8 = 16;
const FLAG_RESTRICTED: u8 = 32;

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct Commands {
    pub nodes: Vec<CommandNode>,
    #[mcr(varint)]
    pub root_index: i32,
}

packet!(Commands, "commands", Clientbound);

#[derive(Debug, Clone, PartialEq)]
pub struct CommandNode {
    pub executable: bool,
    pub restricted: bool,
    pub children: Vec<i32>,
    pub redirect: Option<i32>,
    pub kind: NodeKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NodeKind {
    Root,
    Literal(String),
    Argument {
        name: String,
        parser: ArgumentParser,
        suggestions: Option<Identifier>,
    },
}

impl Encode for CommandNode {
    fn encode(&self, out: &mut Vec<u8>) {
        let mut flags = match self.kind {
            NodeKind::Root => TYPE_ROOT,
            NodeKind::Literal(_) => TYPE_LITERAL,
            NodeKind::Argument { .. } => TYPE_ARGUMENT,
        };
        if self.executable {
            flags |= FLAG_EXECUTABLE;
        }
        if self.redirect.is_some() {
            flags |= FLAG_REDIRECT;
        }
        if matches!(
            &self.kind,
            NodeKind::Argument {
                suggestions: Some(_),
                ..
            }
        ) {
            flags |= FLAG_SUGGESTIONS;
        }
        if self.restricted {
            flags |= FLAG_RESTRICTED;
        }
        flags.encode(out);
        write_varint(out, self.children.len() as i32);
        for &child in &self.children {
            write_varint(out, child);
        }
        if let Some(redirect) = self.redirect {
            write_varint(out, redirect);
        }
        match &self.kind {
            NodeKind::Root => {}
            NodeKind::Literal(name) => name.encode(out),
            NodeKind::Argument {
                name,
                parser,
                suggestions,
            } => {
                name.encode(out);
                parser.encode(out);
                if let Some(id) = suggestions {
                    id.encode(out);
                }
            }
        }
    }
}

impl Decode<'_> for CommandNode {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        let flags = u8::decode(input)?;
        let child_count = read_len(input, input.len())?;
        let mut children = Vec::with_capacity(child_count.min(1024));
        for _ in 0..child_count {
            children.push(read_varint(input)?);
        }
        let redirect = if flags & FLAG_REDIRECT != 0 {
            Some(read_varint(input)?)
        } else {
            None
        };
        let kind = match flags & TYPE_MASK {
            TYPE_ROOT => NodeKind::Root,
            TYPE_LITERAL => NodeKind::Literal(String::decode(input)?),
            TYPE_ARGUMENT => NodeKind::Argument {
                name: String::decode(input)?,
                parser: ArgumentParser::decode(input)?,
                suggestions: if flags & FLAG_SUGGESTIONS != 0 {
                    Some(Identifier::decode(input)?)
                } else {
                    None
                },
            },
            other => {
                return Err(DecodeError::InvalidValue {
                    what: "command node type",
                    value: other.into(),
                });
            }
        };
        Ok(Self {
            executable: flags & FLAG_EXECUTABLE != 0,
            restricted: flags & FLAG_RESTRICTED != 0,
            children,
            redirect,
            kind,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum StringKind {
    SingleWord,
    QuotablePhrase,
    GreedyPhrase,
}

/// Variants follow the `minecraft:command_argument_type` registry order; the wire id is the index.
#[derive(Debug, Clone, PartialEq)]
pub enum ArgumentParser {
    Bool,
    Float { min: Option<f32>, max: Option<f32> },
    Double { min: Option<f64>, max: Option<f64> },
    Integer { min: Option<i32>, max: Option<i32> },
    Long { min: Option<i64>, max: Option<i64> },
    String(StringKind),
    Entity { single: bool, players_only: bool },
    GameProfile,
    BlockPos,
    ColumnPos,
    Vec3,
    Vec2,
    BlockState,
    BlockPredicate,
    ItemStack,
    ItemPredicate,
    TeamColor,
    HexColor,
    Component,
    Style,
    Message,
    NbtCompoundTag,
    NbtTag,
    NbtPath,
    Objective,
    ObjectiveCriteria,
    Operation,
    Particle,
    Angle,
    Rotation,
    ScoreboardSlot,
    ScoreHolder { multiple: bool },
    Swizzle,
    Team,
    ItemSlot,
    ItemSlots,
    ResourceLocation,
    Function,
    EntityAnchor,
    IntRange,
    FloatRange,
    Dimension,
    Gamemode,
    Time { min: i32 },
    ResourceOrTag(Identifier),
    ResourceOrTagKey(Identifier),
    Resource(Identifier),
    ResourceKey(Identifier),
    ResourceSelector(Identifier),
    TemplateMirror,
    TemplateRotation,
    Heightmap,
    LootTable,
    LootPredicate,
    LootModifier,
    ContextFloatProvider,
    ContextIntProvider,
    SlotSource,
    Dialog,
    Feature,
    SwingAnimation,
    Uuid,
}

impl ArgumentParser {
    pub fn id(&self) -> i32 {
        match self {
            Self::Bool => 0,
            Self::Float { .. } => 1,
            Self::Double { .. } => 2,
            Self::Integer { .. } => 3,
            Self::Long { .. } => 4,
            Self::String(_) => 5,
            Self::Entity { .. } => 6,
            Self::GameProfile => 7,
            Self::BlockPos => 8,
            Self::ColumnPos => 9,
            Self::Vec3 => 10,
            Self::Vec2 => 11,
            Self::BlockState => 12,
            Self::BlockPredicate => 13,
            Self::ItemStack => 14,
            Self::ItemPredicate => 15,
            Self::TeamColor => 16,
            Self::HexColor => 17,
            Self::Component => 18,
            Self::Style => 19,
            Self::Message => 20,
            Self::NbtCompoundTag => 21,
            Self::NbtTag => 22,
            Self::NbtPath => 23,
            Self::Objective => 24,
            Self::ObjectiveCriteria => 25,
            Self::Operation => 26,
            Self::Particle => 27,
            Self::Angle => 28,
            Self::Rotation => 29,
            Self::ScoreboardSlot => 30,
            Self::ScoreHolder { .. } => 31,
            Self::Swizzle => 32,
            Self::Team => 33,
            Self::ItemSlot => 34,
            Self::ItemSlots => 35,
            Self::ResourceLocation => 36,
            Self::Function => 37,
            Self::EntityAnchor => 38,
            Self::IntRange => 39,
            Self::FloatRange => 40,
            Self::Dimension => 41,
            Self::Gamemode => 42,
            Self::Time { .. } => 43,
            Self::ResourceOrTag(_) => 44,
            Self::ResourceOrTagKey(_) => 45,
            Self::Resource(_) => 46,
            Self::ResourceKey(_) => 47,
            Self::ResourceSelector(_) => 48,
            Self::TemplateMirror => 49,
            Self::TemplateRotation => 50,
            Self::Heightmap => 51,
            Self::LootTable => 52,
            Self::LootPredicate => 53,
            Self::LootModifier => 54,
            Self::ContextFloatProvider => 55,
            Self::ContextIntProvider => 56,
            Self::SlotSource => 57,
            Self::Dialog => 58,
            Self::Feature => 59,
            Self::SwingAnimation => 60,
            Self::Uuid => 61,
        }
    }
}

fn encode_bounds<T: Encode>(min: &Option<T>, max: &Option<T>, out: &mut Vec<u8>) {
    let flags = u8::from(min.is_some()) | u8::from(max.is_some()) << 1;
    flags.encode(out);
    if let Some(min) = min {
        min.encode(out);
    }
    if let Some(max) = max {
        max.encode(out);
    }
}

fn decode_bounds<T: for<'a> Decode<'a>>(input: &mut &[u8]) -> Result<(Option<T>, Option<T>)> {
    let flags = u8::decode(input)?;
    let min = if flags & 1 != 0 {
        Some(T::decode(input)?)
    } else {
        None
    };
    let max = if flags & 2 != 0 {
        Some(T::decode(input)?)
    } else {
        None
    };
    Ok((min, max))
}

impl Encode for ArgumentParser {
    fn encode(&self, out: &mut Vec<u8>) {
        write_varint(out, self.id());
        match self {
            Self::Float { min, max } => encode_bounds(min, max, out),
            Self::Double { min, max } => encode_bounds(min, max, out),
            Self::Integer { min, max } => encode_bounds(min, max, out),
            Self::Long { min, max } => encode_bounds(min, max, out),
            Self::String(kind) => kind.encode(out),
            Self::Entity {
                single,
                players_only,
            } => (u8::from(*single) | u8::from(*players_only) << 1).encode(out),
            Self::ScoreHolder { multiple } => u8::from(*multiple).encode(out),
            Self::Time { min } => min.encode(out),
            Self::ResourceOrTag(registry)
            | Self::ResourceOrTagKey(registry)
            | Self::Resource(registry)
            | Self::ResourceKey(registry)
            | Self::ResourceSelector(registry) => registry.encode(out),
            _ => {}
        }
    }
}

impl Decode<'_> for ArgumentParser {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        let id = read_varint(input)?;
        Self::decode_properties(id, input)
    }
}

impl ArgumentParser {
    /// The properties that follow parser type `id`.
    pub(crate) fn decode_properties(id: i32, input: &mut &[u8]) -> Result<Self> {
        Ok(match id {
            1 => {
                let (min, max) = decode_bounds(input)?;
                Self::Float { min, max }
            }
            2 => {
                let (min, max) = decode_bounds(input)?;
                Self::Double { min, max }
            }
            3 => {
                let (min, max) = decode_bounds(input)?;
                Self::Integer { min, max }
            }
            4 => {
                let (min, max) = decode_bounds(input)?;
                Self::Long { min, max }
            }
            5 => Self::String(StringKind::decode(input)?),
            6 => {
                let flags = take(input, 1)?[0];
                Self::Entity {
                    single: flags & 1 != 0,
                    players_only: flags & 2 != 0,
                }
            }
            31 => Self::ScoreHolder {
                multiple: take(input, 1)?[0] & 1 != 0,
            },
            43 => Self::Time {
                min: i32::decode(input)?,
            },
            44 => Self::ResourceOrTag(Identifier::decode(input)?),
            45 => Self::ResourceOrTagKey(Identifier::decode(input)?),
            46 => Self::Resource(Identifier::decode(input)?),
            47 => Self::ResourceKey(Identifier::decode(input)?),
            48 => Self::ResourceSelector(Identifier::decode(input)?),
            0 => Self::Bool,
            7 => Self::GameProfile,
            8 => Self::BlockPos,
            9 => Self::ColumnPos,
            10 => Self::Vec3,
            11 => Self::Vec2,
            12 => Self::BlockState,
            13 => Self::BlockPredicate,
            14 => Self::ItemStack,
            15 => Self::ItemPredicate,
            16 => Self::TeamColor,
            17 => Self::HexColor,
            18 => Self::Component,
            19 => Self::Style,
            20 => Self::Message,
            21 => Self::NbtCompoundTag,
            22 => Self::NbtTag,
            23 => Self::NbtPath,
            24 => Self::Objective,
            25 => Self::ObjectiveCriteria,
            26 => Self::Operation,
            27 => Self::Particle,
            28 => Self::Angle,
            29 => Self::Rotation,
            30 => Self::ScoreboardSlot,
            32 => Self::Swizzle,
            33 => Self::Team,
            34 => Self::ItemSlot,
            35 => Self::ItemSlots,
            36 => Self::ResourceLocation,
            37 => Self::Function,
            38 => Self::EntityAnchor,
            39 => Self::IntRange,
            40 => Self::FloatRange,
            41 => Self::Dimension,
            42 => Self::Gamemode,
            49 => Self::TemplateMirror,
            50 => Self::TemplateRotation,
            51 => Self::Heightmap,
            52 => Self::LootTable,
            53 => Self::LootPredicate,
            54 => Self::LootModifier,
            55 => Self::ContextFloatProvider,
            56 => Self::ContextIntProvider,
            57 => Self::SlotSource,
            58 => Self::Dialog,
            59 => Self::Feature,
            60 => Self::SwingAnimation,
            61 => Self::Uuid,
            _ => {
                return Err(DecodeError::InvalidValue {
                    what: "argument parser",
                    value: id.into(),
                });
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use mcr_buf::to_bytes;

    use super::*;
    use crate::test_util::roundtrip;

    fn node(kind: NodeKind, children: Vec<i32>) -> CommandNode {
        CommandNode {
            executable: false,
            restricted: false,
            children,
            redirect: None,
            kind,
        }
    }

    #[test]
    fn graph_bytes() {
        let packet = Commands {
            nodes: vec![
                node(NodeKind::Root, vec![1]),
                node(NodeKind::Literal("say".into()), vec![2]),
                CommandNode {
                    executable: true,
                    restricted: false,
                    children: vec![],
                    redirect: None,
                    kind: NodeKind::Argument {
                        name: "msg".into(),
                        parser: ArgumentParser::Message,
                        suggestions: None,
                    },
                },
            ],
            root_index: 0,
        };
        let bytes = roundtrip(&packet);
        #[rustfmt::skip]
        assert_eq!(bytes, [
            3,
            0, 1, 1,
            1, 1, 2, 3, b's', b'a', b'y',
            2 | 4, 0, 3, b'm', b's', b'g', 20,
            0,
        ]);
    }

    #[test]
    fn redirect_and_suggestions() {
        let packet = Commands {
            nodes: vec![
                node(NodeKind::Root, vec![1]),
                CommandNode {
                    executable: false,
                    restricted: true,
                    children: vec![],
                    redirect: Some(0),
                    kind: NodeKind::Argument {
                        name: "n".into(),
                        parser: ArgumentParser::Integer {
                            min: Some(1),
                            max: None,
                        },
                        suggestions: Some(Identifier::parse("ask_server").unwrap()),
                    },
                },
            ],
            root_index: 0,
        };
        let bytes = roundtrip(&packet);
        let mut expected = vec![2, 0, 1, 1, 2 | 8 | 16 | 32, 0, 0, 1, b'n', 3, 1, 0, 0, 0, 1];
        expected.push(20);
        expected.extend_from_slice(b"minecraft:ask_server");
        expected.push(0);
        assert_eq!(bytes, expected);
    }

    #[test]
    fn parser_properties() {
        let cases = [
            ArgumentParser::Float {
                min: Some(-1.5),
                max: Some(2.5),
            },
            ArgumentParser::Double {
                min: None,
                max: Some(9.0),
            },
            ArgumentParser::Long {
                min: Some(i64::MIN),
                max: Some(i64::MAX),
            },
            ArgumentParser::String(StringKind::GreedyPhrase),
            ArgumentParser::Entity {
                single: true,
                players_only: true,
            },
            ArgumentParser::ScoreHolder { multiple: true },
            ArgumentParser::Time { min: 1 },
            ArgumentParser::ResourceOrTag(Identifier::parse("block").unwrap()),
            ArgumentParser::ResourceOrTagKey(Identifier::parse("item").unwrap()),
            ArgumentParser::Resource(Identifier::parse("biome").unwrap()),
            ArgumentParser::ResourceKey(Identifier::parse("dimension").unwrap()),
            ArgumentParser::ResourceSelector(Identifier::parse("loot_table").unwrap()),
        ];
        for case in cases {
            roundtrip(&case);
        }
        assert_eq!(
            to_bytes(&ArgumentParser::Entity {
                single: true,
                players_only: true
            }),
            [6, 3]
        );
        assert_eq!(
            to_bytes(&ArgumentParser::Float {
                min: Some(1.0),
                max: None
            }),
            [1, 1, 0x3f, 0x80, 0, 0]
        );
        assert_eq!(
            to_bytes(&ArgumentParser::Double {
                min: None,
                max: Some(9.0)
            }),
            [2, 2, 0x40, 0x22, 0, 0, 0, 0, 0, 0]
        );
        #[rustfmt::skip]
        assert_eq!(
            to_bytes(&ArgumentParser::Long { min: Some(i64::MIN), max: Some(-1) }),
            [4, 3, 0x80, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]
        );
        assert_eq!(
            to_bytes(&ArgumentParser::String(StringKind::GreedyPhrase)),
            [5, 2]
        );
        assert_eq!(
            to_bytes(&ArgumentParser::ScoreHolder { multiple: true }),
            [31, 1]
        );
        assert_eq!(to_bytes(&ArgumentParser::Time { min: 1 }), [43, 0, 0, 0, 1]);
        let mut expected = vec![47, 19];
        expected.extend_from_slice(b"minecraft:dimension");
        assert_eq!(
            to_bytes(&ArgumentParser::ResourceKey(
                Identifier::parse("dimension").unwrap()
            )),
            expected
        );
    }

    #[test]
    fn every_registry_id_decodes() {
        for id in 0..62 {
            let mut bytes = vec![id as u8];
            bytes.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            let mut input = bytes.as_slice();
            let parser = ArgumentParser::decode(&mut input).unwrap_or_else(|e| {
                panic!("id {id}: {e}");
            });
            assert_eq!(parser.id(), id);
        }
        assert!(ArgumentParser::decode(&mut &[62u8][..]).is_err());
    }

    #[test]
    fn rejects_bad_node_type() {
        assert!(CommandNode::decode(&mut &[3u8, 0][..]).is_err());
    }
}
