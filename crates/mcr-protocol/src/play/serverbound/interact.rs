use mcr_buf::{BlockPos, Decode, Encode};

use super::shared::{Facing, Hand, facing_byte};
use crate::packet;
use crate::types::{Vec3d, Vec3f};

mod lp_vec3 {
    use mcr_buf::{Decode, Result, read_varint, take, write_varint};

    use crate::types::Vec3d;

    const ABS_MAX: f64 = 1.7179869183e10;
    const ABS_MIN: f64 = 3.051944088384301e-5;
    const MAX_QUANTIZED: f64 = 32766.0;

    fn sanitize(v: f64) -> f64 {
        if v.is_nan() {
            0.0
        } else {
            v.clamp(-ABS_MAX, ABS_MAX)
        }
    }

    fn pack(v: f64) -> i64 {
        ((v * 0.5 + 0.5) * MAX_QUANTIZED + 0.5).floor() as i64
    }

    fn unpack(v: i64) -> f64 {
        ((v & 32767) as f64).min(MAX_QUANTIZED) * 2.0 / MAX_QUANTIZED - 1.0
    }

    pub fn encode(value: &Vec3d, out: &mut Vec<u8>) {
        let (x, y, z) = (sanitize(value.x), sanitize(value.y), sanitize(value.z));
        let length = x.abs().max(y.abs().max(z.abs()));
        if length < ABS_MIN {
            out.push(0);
            return;
        }
        let scale = length.ceil() as i64;
        let partial = scale & 3 != scale;
        let markers = if partial { scale & 3 | 4 } else { scale };
        let scale_f = scale as f64;
        let buffer =
            markers | pack(x / scale_f) << 3 | pack(y / scale_f) << 18 | pack(z / scale_f) << 33;
        out.push(buffer as u8);
        out.push((buffer >> 8) as u8);
        out.extend_from_slice(&((buffer >> 16) as i32).to_be_bytes());
        if partial {
            write_varint(out, (scale >> 2) as i32);
        }
    }

    pub fn decode(input: &mut &[u8]) -> Result<Vec3d> {
        let lowest = i64::from(u8::decode(input)?);
        if lowest == 0 {
            return Ok(Vec3d::default());
        }
        let middle = i64::from(u8::decode(input)?);
        let highest = i64::from(u32::from_be_bytes(take(input, 4)?.try_into().unwrap()));
        let buffer = highest << 16 | middle << 8 | lowest;
        let mut scale = lowest & 3;
        if lowest & 4 != 0 {
            scale |= i64::from(read_varint(input)? as u32) << 2;
        }
        let scale = scale as f64;
        Ok(Vec3d {
            x: unpack(buffer >> 3) * scale,
            y: unpack(buffer >> 18) * scale,
            z: unpack(buffer >> 33) * scale,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Encode, Decode)]
pub struct Interact {
    #[mcr(varint)]
    pub entity_id: i32,
    pub hand: Hand,
    #[mcr(with = lp_vec3)]
    pub location: Vec3d,
    pub using_secondary_action: bool,
}

packet!(Interact, "interact", Serverbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub struct Attack {
    #[mcr(varint)]
    pub entity_id: i32,
}

packet!(Attack, "attack", Serverbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Encode, Decode)]
pub struct Punch;

packet!(Punch, "punch", Serverbound);

#[derive(Debug, Clone, Copy, PartialEq, Encode, Decode)]
pub struct BlockHit {
    pub pos: BlockPos,
    pub face: Facing,
    pub cursor: Vec3f,
    pub inside: bool,
    pub world_border_hit: bool,
}

impl BlockHit {
    pub fn new(pos: BlockPos, face: Facing, cursor: Vec3f, inside: bool) -> Self {
        Self {
            pos,
            face,
            cursor,
            inside,
            world_border_hit: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Encode, Decode)]
pub struct UseItemOn {
    pub hand: Hand,
    pub hit: BlockHit,
    #[mcr(varint)]
    pub sequence: i32,
}

impl UseItemOn {
    pub fn new(hand: Hand, hit: BlockHit, sequence: i32) -> Self {
        Self {
            hand,
            hit,
            sequence,
        }
    }
}

packet!(UseItemOn, "use_item_on", Serverbound);

#[derive(Debug, Clone, Copy, PartialEq, Encode, Decode)]
pub struct UseItem {
    pub hand: Hand,
    #[mcr(varint)]
    pub sequence: i32,
    pub y_rot: f32,
    pub x_rot: f32,
}

packet!(UseItem, "use_item", Serverbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub enum PlayerActionKind {
    StartDestroyBlock,
    ChangeDestroyDirection,
    AbortDestroyBlock,
    StopDestroyBlock,
    DropAllItems,
    DropItem,
    ReleaseUseItem,
    SwapItemWithOffhand,
    Stab,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub struct PlayerAction {
    pub action: PlayerActionKind,
    pub pos: BlockPos,
    #[mcr(with = facing_byte)]
    pub face: Facing,
    #[mcr(varint)]
    pub sequence: i32,
}

impl PlayerAction {
    pub fn new(action: PlayerActionKind, pos: BlockPos, face: Facing, sequence: i32) -> Self {
        Self {
            action,
            pos,
            face,
            sequence,
        }
    }
}

packet!(PlayerAction, "player_action", Serverbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub enum PlayerCommandKind {
    StopSleeping,
    StartSprinting,
    StopSprinting,
    StartRidingJump,
    StopRidingJump,
    OpenInventory,
    StartFallFlying,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub struct PlayerCommand {
    #[mcr(varint)]
    pub entity_id: i32,
    pub action: PlayerCommandKind,
    #[mcr(varint)]
    pub data: i32,
}

packet!(PlayerCommand, "player_command", Serverbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub struct PickItemFromBlock {
    pub pos: BlockPos,
    pub include_data: bool,
}

packet!(PickItemFromBlock, "pick_item_from_block", Serverbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub struct PickItemFromEntity {
    #[mcr(varint)]
    pub id: i32,
    pub include_data: bool,
}

packet!(PickItemFromEntity, "pick_item_from_entity", Serverbound);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::play::serverbound::shared::roundtrips;
    use crate::test_util::roundtrip;

    roundtrips! {
        attack: Attack { entity_id: 12 },
        punch: Punch,
        command: PlayerCommand { entity_id: 3, action: PlayerCommandKind::StartFallFlying, data: 0 },
        pick_block: PickItemFromBlock { pos: BlockPos::new(1, 2, 3), include_data: true },
        pick_entity: PickItemFromEntity { id: 99, include_data: false },
        use_item: UseItem { hand: Hand::Off, sequence: 4, y_rot: 1.5, x_rot: -2.5 },
        far_interact: Interact {
            entity_id: 1,
            hand: Hand::Main,
            location: Vec3d { x: 1.0, y: 0.0, z: -1.0 },
            using_secondary_action: false,
        },
    }

    #[test]
    fn interact_bytes() {
        let packet = Interact {
            entity_id: 5,
            hand: Hand::Main,
            location: Vec3d {
                x: 1.0,
                y: 0.0,
                z: -1.0,
            },
            using_secondary_action: true,
        };
        assert_eq!(
            roundtrip(&packet),
            [5, 0, 0xf1, 0xff, 0x00, 0x00, 0xff, 0xff, 1]
        );
    }

    #[test]
    fn zero_and_large_vectors() {
        let zero = Interact {
            entity_id: 0,
            hand: Hand::Off,
            location: Vec3d::default(),
            using_secondary_action: false,
        };
        assert_eq!(roundtrip(&zero), [0, 1, 0, 0]);

        let far = Interact {
            location: Vec3d {
                x: 8.0,
                y: 0.0,
                z: 0.0,
            },
            ..zero
        };
        assert_eq!(
            roundtrip(&far),
            [0, 1, 0xf4, 0xff, 0x7f, 0xfe, 0xff, 0xff, 2, 0]
        );
    }

    #[test]
    fn small_packet_bytes() {
        assert_eq!(roundtrip(&Attack { entity_id: 300 }), [0xac, 0x02]);
        assert!(roundtrip(&Punch).is_empty());
        assert_eq!(
            roundtrip(&UseItem {
                hand: Hand::Off,
                sequence: 4,
                y_rot: 1.5,
                x_rot: -2.5
            }),
            [1, 4, 0x3f, 0xc0, 0, 0, 0xc0, 0x20, 0, 0]
        );
        assert_eq!(
            roundtrip(&PlayerCommand {
                entity_id: 3,
                action: PlayerCommandKind::StartFallFlying,
                data: 0
            }),
            [3, 6, 0]
        );
        assert_eq!(
            roundtrip(&PickItemFromBlock {
                pos: BlockPos::new(0, 5, 0),
                include_data: true
            }),
            [0, 0, 0, 0, 0, 0, 0, 5, 1]
        );
        assert_eq!(
            roundtrip(&PickItemFromEntity {
                id: 99,
                include_data: false
            }),
            [99, 0]
        );
    }

    #[test]
    fn use_item_on_bytes() {
        let packet = UseItemOn::new(
            Hand::Main,
            BlockHit::new(
                BlockPos::new(1, 0, 0),
                Facing::Up,
                Vec3f {
                    x: 0.5,
                    y: 1.0,
                    z: 0.0,
                },
                true,
            ),
            2,
        );
        let mut expected = vec![0, 0, 0, 0, 0x40, 0, 0, 0, 0, 1];
        expected.extend_from_slice(&0.5f32.to_be_bytes());
        expected.extend_from_slice(&1.0f32.to_be_bytes());
        expected.extend_from_slice(&0.0f32.to_be_bytes());
        expected.extend_from_slice(&[1, 0, 2]);
        assert_eq!(roundtrip(&packet), expected);
    }

    #[test]
    fn player_action_bytes() {
        let packet = PlayerAction::new(
            PlayerActionKind::StopDestroyBlock,
            BlockPos::new(0, 5, 0),
            Facing::West,
            300,
        );
        assert_eq!(
            roundtrip(&packet),
            [3, 0, 0, 0, 0, 0, 0, 0, 5, 4, 0xac, 0x02]
        );
    }
}
