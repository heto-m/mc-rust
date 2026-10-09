mod chat;
mod info;

use mcr_buf::{Decode, DecodeError, Encode, Identifier, Result, VarInt, read_varint, write_varint};

pub use self::{
    chat::{
        BoundChatType, ChatParameter, ChatType, ChatTypeDecoration, CompletionAction,
        CustomChatCompletions, DeleteChat, DisguisedChat, FilterMask, PackedSignature, PlayerChat,
        SignedMessageBody, SystemChat,
    },
    info::{
        PlayerInfoAction, PlayerInfoEntry, PlayerInfoRemove, PlayerInfoUpdate, PlayerProfile,
        RemoteChatSession,
    },
};
use super::ui::Pairs;
use crate::{
    packet,
    types::{
        Component, Vec3d,
        component::GlobalPos,
        game::{Difficulty, GameType},
    },
};

/// `0` is absent, otherwise the game type id plus one.
mod optional_game_type {
    use super::*;

    pub fn encode(value: &Option<GameType>, out: &mut Vec<u8>) {
        write_varint(out, value.map_or(0, |mode| mode as i32 + 1));
    }

    pub fn decode(input: &mut &[u8]) -> Result<Option<GameType>> {
        Ok(Some(match read_varint(input)? {
            0 => return Ok(None),
            1 => GameType::Survival,
            2 => GameType::Creative,
            3 => GameType::Adventure,
            4 => GameType::Spectator,
            n => {
                return Err(DecodeError::InvalidValue {
                    what: "game type",
                    value: n.into(),
                });
            }
        }))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct CommonPlayerSpawnInfo {
    #[mcr(varint)]
    pub dimension_type: i32,
    pub dimension: Identifier,
    pub seed: i64,
    pub game_type: GameType,
    #[mcr(with = optional_game_type)]
    pub previous_game_type: Option<GameType>,
    pub is_debug: bool,
    pub is_flat: bool,
    pub last_death_location: Option<GlobalPos>,
    #[mcr(varint)]
    pub portal_cooldown: i32,
    #[mcr(varint)]
    pub sea_level: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct Login {
    pub player_id: i32,
    pub hardcore: bool,
    pub levels: Vec<Identifier>,
    #[mcr(varint)]
    pub max_players: i32,
    #[mcr(varint)]
    pub chunk_radius: i32,
    #[mcr(varint)]
    pub simulation_distance: i32,
    pub reduced_debug_info: bool,
    pub show_death_screen: bool,
    pub do_limited_crafting: bool,
    pub spawn_info: CommonPlayerSpawnInfo,
    pub online_mode: bool,
    pub enforces_secure_chat: bool,
}

packet!(Login, "login", Clientbound);

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct Respawn {
    pub spawn_info: CommonPlayerSpawnInfo,
    pub data_to_keep: u8,
}

impl Respawn {
    pub const KEEP_ATTRIBUTE_MODIFIERS: u8 = 1;
    pub const KEEP_ENTITY_DATA: u8 = 2;
    pub const KEEP_ALL_DATA: u8 = 3;
}

packet!(Respawn, "respawn", Clientbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct ChangeDifficulty {
    pub difficulty: Difficulty,
    pub locked: bool,
}

packet!(ChangeDifficulty, "change_difficulty", Clientbound);

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PlayerAbilities {
    pub invulnerable: bool,
    pub flying: bool,
    pub can_fly: bool,
    pub instabuild: bool,
    pub flying_speed: f32,
    pub walking_speed: f32,
}

packet!(PlayerAbilities, "player_abilities", Clientbound);

impl Encode for PlayerAbilities {
    fn encode(&self, out: &mut Vec<u8>) {
        out.push(
            u8::from(self.invulnerable)
                | u8::from(self.flying) << 1
                | u8::from(self.can_fly) << 2
                | u8::from(self.instabuild) << 3,
        );
        self.flying_speed.encode(out);
        self.walking_speed.encode(out);
    }
}

impl Decode<'_> for PlayerAbilities {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        let flags = u8::decode(input)?;
        Ok(Self {
            invulnerable: flags & 1 != 0,
            flying: flags & 2 != 0,
            can_fly: flags & 4 != 0,
            instabuild: flags & 8 != 0,
            flying_speed: f32::decode(input)?,
            walking_speed: f32::decode(input)?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RelativeFlags(u16);

impl RelativeFlags {
    pub const X: Self = Self(1);
    pub const Y: Self = Self(1 << 1);
    pub const Z: Self = Self(1 << 2);
    pub const Y_ROT: Self = Self(1 << 3);
    pub const X_ROT: Self = Self(1 << 4);
    pub const DELTA_X: Self = Self(1 << 5);
    pub const DELTA_Y: Self = Self(1 << 6);
    pub const DELTA_Z: Self = Self(1 << 7);
    pub const ROTATE_DELTA: Self = Self(1 << 8);
    const ALL_BITS: i32 = 0x1ff;

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn bits(self) -> u16 {
        self.0
    }
}

impl Encode for RelativeFlags {
    fn encode(&self, out: &mut Vec<u8>) {
        i32::from(self.0).encode(out);
    }
}

impl Decode<'_> for RelativeFlags {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        Ok(Self((i32::decode(input)? & Self::ALL_BITS) as u16))
    }
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct PlayerPosition {
    #[mcr(varint)]
    pub id: i32,
    pub position: Vec3d,
    pub delta_movement: Vec3d,
    pub y_rot: f32,
    pub x_rot: f32,
    pub relatives: RelativeFlags,
}

packet!(PlayerPosition, "player_position", Clientbound);

#[derive(Debug, Clone, Copy, PartialEq, Encode, Decode)]
pub struct PlayerRotation {
    pub y_rot: f32,
    pub relative_y: bool,
    pub x_rot: f32,
    pub relative_x: bool,
}

packet!(PlayerRotation, "player_rotation", Clientbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum EntityAnchor {
    Feet,
    Eyes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct LookAtEntity {
    #[mcr(varint)]
    pub entity_id: i32,
    pub anchor: EntityAnchor,
}

#[derive(Debug, Clone, Copy, PartialEq, Encode, Decode)]
pub struct PlayerLookAt {
    pub from_anchor: EntityAnchor,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub entity: Option<LookAtEntity>,
}

packet!(PlayerLookAt, "player_look_at", Clientbound);

#[derive(Debug, Clone, Copy, PartialEq, Encode, Decode)]
pub struct SetHealth {
    pub health: f32,
    #[mcr(varint)]
    pub food: i32,
    pub saturation: f32,
}

packet!(SetHealth, "set_health", Clientbound);

#[derive(Debug, Clone, Copy, PartialEq, Encode, Decode)]
pub struct SetExperience {
    pub progress: f32,
    #[mcr(varint)]
    pub level: i32,
    #[mcr(varint)]
    pub total: i32,
}

packet!(SetExperience, "set_experience", Clientbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct SetHeldSlot {
    #[mcr(varint)]
    pub slot: i32,
}

packet!(SetHeldSlot, "set_held_slot", Clientbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct PlayerCombatEnd {
    #[mcr(varint)]
    pub duration: i32,
}

packet!(PlayerCombatEnd, "player_combat_end", Clientbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct PlayerCombatEnter;

packet!(PlayerCombatEnter, "player_combat_enter", Clientbound);

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct PlayerCombatKill {
    #[mcr(varint)]
    pub player_id: i32,
    pub message: Component,
}

packet!(PlayerCombatKill, "player_combat_kill", Clientbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct StartConfiguration;

packet!(StartConfiguration, "start_configuration", Clientbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct LowDiskSpaceWarning;

packet!(LowDiskSpaceWarning, "low_disk_space_warning", Clientbound);

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct ServerData {
    pub motd: Component,
    pub icon: Option<Vec<u8>>,
}

packet!(ServerData, "server_data", Clientbound);

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct GameRuleValues {
    pub values: Pairs<Identifier, String>,
}

packet!(GameRuleValues, "game_rule_values", Clientbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct Stat {
    #[mcr(varint)]
    pub stat_type: i32,
    #[mcr(varint)]
    pub value: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct AwardStats {
    pub stats: Pairs<Stat, VarInt>,
}

packet!(AwardStats, "award_stats", Clientbound);

#[cfg(test)]
mod tests {
    use mcr_buf::BlockPos;

    use super::*;
    use crate::{common::PostEffects, test_util::roundtrip};

    fn spawn_info() -> CommonPlayerSpawnInfo {
        CommonPlayerSpawnInfo {
            dimension_type: 2,
            dimension: Identifier::parse("overworld").unwrap(),
            seed: 1,
            game_type: GameType::Creative,
            previous_game_type: Some(GameType::Survival),
            is_debug: false,
            is_flat: true,
            last_death_location: None,
            portal_cooldown: 0,
            sea_level: 63,
        }
    }

    fn spawn_bytes() -> Vec<u8> {
        let mut bytes = vec![2, 19];
        bytes.extend_from_slice(b"minecraft:overworld");
        bytes.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 0, 1, 0, 0, 63]);
        bytes
    }

    #[test]
    fn spawn_info_bytes() {
        assert_eq!(roundtrip(&spawn_info()), spawn_bytes());
        let mut with_death = spawn_info();
        with_death.previous_game_type = None;
        with_death.last_death_location = Some(GlobalPos {
            dimension: Identifier::parse("the_end").unwrap(),
            pos: BlockPos::new(0, 0, 1),
        });
        roundtrip(&with_death);
        assert!(CommonPlayerSpawnInfo::decode(&mut spawn_bytes()[..30].as_ref()).is_err());
    }

    #[test]
    fn login_bytes() {
        let packet = Login {
            player_id: 7,
            hardcore: false,
            levels: vec![Identifier::parse("overworld").unwrap()],
            max_players: 20,
            chunk_radius: 10,
            simulation_distance: 8,
            reduced_debug_info: false,
            show_death_screen: true,
            do_limited_crafting: false,
            spawn_info: spawn_info(),
            online_mode: true,
            enforces_secure_chat: false,
        };
        let mut expected = vec![0, 0, 0, 7, 0, 1, 19];
        expected.extend_from_slice(b"minecraft:overworld");
        expected.extend_from_slice(&[20, 10, 8, 0, 1, 0]);
        expected.extend(spawn_bytes());
        expected.extend_from_slice(&[1, 0]);
        assert_eq!(roundtrip(&packet), expected);
    }

    #[test]
    fn respawn_bytes() {
        let mut expected = spawn_bytes();
        expected.push(3);
        assert_eq!(
            roundtrip(&Respawn {
                spawn_info: spawn_info(),
                data_to_keep: Respawn::KEEP_ALL_DATA,
            }),
            expected
        );
        let mut died = spawn_info();
        died.previous_game_type = None;
        died.last_death_location = Some(GlobalPos {
            dimension: Identifier::parse("the_end").unwrap(),
            pos: BlockPos::new(0, 0, 1),
        });
        let mut expected = vec![2, 19];
        expected.extend_from_slice(b"minecraft:overworld");
        expected.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 1, 1, 0, 0, 1, 1, 17]);
        expected.extend_from_slice(b"minecraft:the_end");
        expected.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0x10, 0, 0, 63, 1]);
        assert_eq!(
            roundtrip(&Respawn {
                spawn_info: died,
                data_to_keep: Respawn::KEEP_ATTRIBUTE_MODIFIERS,
            }),
            expected
        );
    }

    #[test]
    fn vitals() {
        assert_eq!(
            roundtrip(&SetHealth {
                health: 20.0,
                food: 20,
                saturation: 5.0,
            }),
            [0x41, 0xa0, 0, 0, 20, 0x40, 0xa0, 0, 0]
        );
        assert_eq!(
            roundtrip(&SetExperience {
                progress: 0.5,
                level: 30,
                total: 300,
            }),
            [0x3f, 0, 0, 0, 30, 0xac, 2]
        );
        assert_eq!(roundtrip(&SetHeldSlot { slot: 4 }), [4]);
        assert_eq!(roundtrip(&PlayerCombatEnd { duration: 300 }), [0xac, 2]);
    }

    #[test]
    fn unit_packets() {
        assert!(roundtrip(&PlayerCombatEnter).is_empty());
        assert!(roundtrip(&StartConfiguration).is_empty());
        assert!(roundtrip(&LowDiskSpaceWarning).is_empty());
    }

    #[test]
    fn abilities_bytes() {
        let packet = PlayerAbilities {
            invulnerable: true,
            flying: false,
            can_fly: true,
            instabuild: true,
            flying_speed: 0.05,
            walking_speed: 0.1,
        };
        let bytes = roundtrip(&packet);
        assert_eq!(bytes[0], 0b1101);
        assert_eq!(bytes.len(), 9);
    }

    #[test]
    fn position_and_rotation() {
        let packet = PlayerPosition {
            id: 1,
            position: Vec3d {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            },
            delta_movement: Vec3d::default(),
            y_rot: 90.0,
            x_rot: 0.0,
            relatives: RelativeFlags::X.union(RelativeFlags::ROTATE_DELTA),
        };
        let mut expected = vec![1];
        expected.extend_from_slice(&1.0f64.to_be_bytes());
        expected.extend_from_slice(&2.0f64.to_be_bytes());
        expected.extend_from_slice(&3.0f64.to_be_bytes());
        expected.extend_from_slice(&[0; 24]);
        expected.extend_from_slice(&90.0f32.to_be_bytes());
        expected.extend_from_slice(&[0; 4]);
        expected.extend_from_slice(&[0, 0, 1, 1]);
        assert_eq!(roundtrip(&packet), expected);
        assert!(packet.relatives.contains(RelativeFlags::X));
        assert_eq!(
            RelativeFlags::decode(&mut &[0xff, 0xff, 0xff, 0xff][..])
                .unwrap()
                .bits(),
            0x1ff
        );
        assert_eq!(
            roundtrip(&PlayerRotation {
                y_rot: 1.0,
                relative_y: true,
                x_rot: 0.0,
                relative_x: false,
            }),
            [0x3f, 0x80, 0, 0, 1, 0, 0, 0, 0, 0]
        );
    }

    #[test]
    fn look_at() {
        let at_point = PlayerLookAt {
            from_anchor: EntityAnchor::Eyes,
            x: 0.0,
            y: 0.0,
            z: 0.0,
            entity: None,
        };
        let mut expected = vec![1];
        expected.extend_from_slice(&[0; 24]);
        expected.push(0);
        assert_eq!(roundtrip(&at_point), expected);
        let at_entity = PlayerLookAt {
            entity: Some(LookAtEntity {
                entity_id: 5,
                anchor: EntityAnchor::Feet,
            }),
            ..at_point
        };
        let mut expected = vec![1];
        expected.extend_from_slice(&[0; 24]);
        expected.extend_from_slice(&[1, 5, 0]);
        assert_eq!(roundtrip(&at_entity), expected);
    }

    #[test]
    fn misc_packets() {
        assert_eq!(
            roundtrip(&ChangeDifficulty {
                difficulty: Difficulty::Hard,
                locked: true,
            }),
            [3, 1]
        );
        assert_eq!(
            roundtrip(&PostEffects {
                post_effects: vec![Identifier::parse("a:b").unwrap()],
            }),
            [1, 3, b'a', b':', b'b']
        );
        let rules = GameRuleValues {
            values: Pairs(vec![(Identifier::parse("a:b").unwrap(), "true".into())]),
        };
        assert_eq!(
            roundtrip(&rules),
            [1, 3, b'a', b':', b'b', 4, b't', b'r', b'u', b'e']
        );
        let stats = AwardStats {
            stats: Pairs(vec![(
                Stat {
                    stat_type: 8,
                    value: 2,
                },
                VarInt(300),
            )]),
        };
        assert_eq!(roundtrip(&stats), [1, 8, 2, 0xac, 2]);
    }

    #[test]
    fn component_packets() {
        assert_eq!(
            roundtrip(&PlayerCombatKill {
                player_id: 300,
                message: Component::text("x"),
            }),
            [0xac, 2, 8, 0, 1, b'x']
        );
        roundtrip(&ServerData {
            motd: Component::default(),
            icon: Some(vec![1, 2, 3]),
        });
    }
}
