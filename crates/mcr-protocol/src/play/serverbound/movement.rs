use mcr_buf::{Decode, Encode, Result};

use super::shared::FlagByte;
use crate::packet;
use crate::types::Vec3d;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct MoveFlags {
    pub on_ground: bool,
    pub horizontal_collision: bool,
}

impl MoveFlags {
    pub const fn new(on_ground: bool, horizontal_collision: bool) -> Self {
        Self {
            on_ground,
            horizontal_collision,
        }
    }
}

impl Encode for MoveFlags {
    fn encode(&self, out: &mut Vec<u8>) {
        out.push(u8::from(self.on_ground) | u8::from(self.horizontal_collision) << 1);
    }
}

impl Decode<'_> for MoveFlags {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        let flags = FlagByte::decode(input)?;
        Ok(Self::new(flags.bit(0), flags.bit(1)))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Encode, Decode)]
pub struct MovePlayerPos {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub flags: MoveFlags,
}

impl MovePlayerPos {
    pub const fn new(x: f64, y: f64, z: f64, on_ground: bool, horizontal_collision: bool) -> Self {
        Self {
            x,
            y,
            z,
            flags: MoveFlags::new(on_ground, horizontal_collision),
        }
    }
}

packet!(MovePlayerPos, "move_player_pos", Serverbound);

#[derive(Debug, Clone, Copy, PartialEq, Encode, Decode)]
pub struct MovePlayerPosRot {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub y_rot: f32,
    pub x_rot: f32,
    pub flags: MoveFlags,
}

impl MovePlayerPosRot {
    pub const fn new(
        x: f64,
        y: f64,
        z: f64,
        y_rot: f32,
        x_rot: f32,
        on_ground: bool,
        horizontal_collision: bool,
    ) -> Self {
        Self {
            x,
            y,
            z,
            y_rot,
            x_rot,
            flags: MoveFlags::new(on_ground, horizontal_collision),
        }
    }
}

packet!(MovePlayerPosRot, "move_player_pos_rot", Serverbound);

#[derive(Debug, Clone, Copy, PartialEq, Encode, Decode)]
pub struct MovePlayerRot {
    pub y_rot: f32,
    pub x_rot: f32,
    pub flags: MoveFlags,
}

impl MovePlayerRot {
    pub const fn new(y_rot: f32, x_rot: f32, on_ground: bool, horizontal_collision: bool) -> Self {
        Self {
            y_rot,
            x_rot,
            flags: MoveFlags::new(on_ground, horizontal_collision),
        }
    }
}

packet!(MovePlayerRot, "move_player_rot", Serverbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct MovePlayerStatusOnly {
    pub flags: MoveFlags,
}

impl MovePlayerStatusOnly {
    pub const fn new(on_ground: bool, horizontal_collision: bool) -> Self {
        Self {
            flags: MoveFlags::new(on_ground, horizontal_collision),
        }
    }
}

packet!(MovePlayerStatusOnly, "move_player_status_only", Serverbound);

#[derive(Debug, Clone, Copy, PartialEq, Encode, Decode)]
pub struct MoveVehicle {
    pub position: Vec3d,
    pub y_rot: f32,
    pub x_rot: f32,
    pub on_ground: bool,
}

packet!(MoveVehicle, "move_vehicle", Serverbound);

#[derive(Debug, Clone, Copy, PartialEq, Encode, Decode)]
pub struct AcceptTeleportation {
    #[mcr(varint)]
    pub id: i32,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub y_rot: f32,
    pub x_rot: f32,
}

packet!(AcceptTeleportation, "accept_teleportation", Serverbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct PaddleBoat {
    pub left: bool,
    pub right: bool,
}

packet!(PaddleBoat, "paddle_boat", Serverbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct PlayerInput {
    pub forward: bool,
    pub backward: bool,
    pub left: bool,
    pub right: bool,
    pub jump: bool,
    pub shift: bool,
    pub sprint: bool,
}

impl Encode for PlayerInput {
    fn encode(&self, out: &mut Vec<u8>) {
        let bits = [
            self.forward,
            self.backward,
            self.left,
            self.right,
            self.jump,
            self.shift,
            self.sprint,
        ];
        let flags = bits
            .iter()
            .enumerate()
            .fold(FlagByte(0), |f, (i, &on)| f.with(i as u8, on));
        flags.encode(out);
    }
}

impl Decode<'_> for PlayerInput {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        let f = FlagByte::decode(input)?;
        Ok(Self {
            forward: f.bit(0),
            backward: f.bit(1),
            left: f.bit(2),
            right: f.bit(3),
            jump: f.bit(4),
            shift: f.bit(5),
            sprint: f.bit(6),
        })
    }
}

packet!(PlayerInput, "player_input", Serverbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PlayerAbilities {
    pub flying: bool,
}

impl Encode for PlayerAbilities {
    fn encode(&self, out: &mut Vec<u8>) {
        FlagByte(0).with(1, self.flying).encode(out);
    }
}

impl Decode<'_> for PlayerAbilities {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        Ok(Self {
            flying: FlagByte::decode(input)?.bit(1),
        })
    }
}

packet!(PlayerAbilities, "player_abilities", Serverbound);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::play::serverbound::shared::roundtrips;
    use crate::test_util::roundtrip;

    roundtrips! {
        pos: MovePlayerPos::new(1.0, 64.0, -3.5, true, false),
        rot: MovePlayerRot::new(90.0, -10.0, false, true),
        status: MovePlayerStatusOnly::new(true, true),
        vehicle: MoveVehicle { position: Vec3d { x: 1.0, y: 2.0, z: 3.0 }, y_rot: 1.0, x_rot: 2.0, on_ground: true },
        teleport: AcceptTeleportation { id: 300, x: 1.0, y: 2.0, z: 3.0, y_rot: 4.0, x_rot: 5.0 },
        paddle: PaddleBoat { left: true, right: false },
        input: PlayerInput { forward: true, sprint: true, ..Default::default() },
        abilities: PlayerAbilities { flying: true },
    }

    #[test]
    fn pos_rot_bytes() {
        let bytes = roundtrip(&MovePlayerPosRot::new(
            1.0, 2.0, 3.0, 90.0, -45.0, true, false,
        ));
        let mut expected = Vec::new();
        for d in [1.0f64, 2.0, 3.0] {
            expected.extend_from_slice(&d.to_be_bytes());
        }
        expected.extend_from_slice(&90.0f32.to_be_bytes());
        expected.extend_from_slice(&(-45.0f32).to_be_bytes());
        expected.push(1);
        assert_eq!(bytes, expected);
    }

    const ONE: [u8; 8] = [0x3f, 0xf0, 0, 0, 0, 0, 0, 0];
    const TWO: [u8; 8] = [0x40, 0, 0, 0, 0, 0, 0, 0];
    const THREE: [u8; 8] = [0x40, 0x08, 0, 0, 0, 0, 0, 0];

    #[test]
    fn pos_and_rot_bytes() {
        let expected = [
            &ONE[..],
            &[0x40, 0x50, 0, 0, 0, 0, 0, 0],
            &[0xc0, 0x0c, 0, 0, 0, 0, 0, 0],
            &[1],
        ]
        .concat();
        assert_eq!(
            roundtrip(&MovePlayerPos::new(1.0, 64.0, -3.5, true, false)),
            expected
        );
        assert_eq!(
            roundtrip(&MovePlayerRot::new(90.0, -10.0, false, true)),
            [0x42, 0xb4, 0, 0, 0xc1, 0x20, 0, 0, 2]
        );
    }

    #[test]
    fn vehicle_and_teleport_bytes() {
        let vehicle = MoveVehicle {
            position: Vec3d {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            },
            y_rot: 1.0,
            x_rot: 2.0,
            on_ground: true,
        };
        let expected = [
            &ONE[..],
            &TWO,
            &THREE,
            &[0x3f, 0x80, 0, 0, 0x40, 0, 0, 0, 1],
        ]
        .concat();
        assert_eq!(roundtrip(&vehicle), expected);

        let teleport = AcceptTeleportation {
            id: 300,
            x: 1.0,
            y: 2.0,
            z: 3.0,
            y_rot: 4.0,
            x_rot: 5.0,
        };
        let expected = [
            &[0xac, 0x02][..],
            &ONE,
            &TWO,
            &THREE,
            &[0x40, 0x80, 0, 0, 0x40, 0xa0, 0, 0],
        ]
        .concat();
        assert_eq!(roundtrip(&teleport), expected);
        assert_eq!(
            roundtrip(&PaddleBoat {
                left: true,
                right: false
            }),
            [1, 0]
        );
    }

    #[test]
    fn flags_byte() {
        assert_eq!(roundtrip(&MovePlayerStatusOnly::new(true, true)), [3]);
        assert_eq!(roundtrip(&MovePlayerStatusOnly::new(false, true)), [2]);
        assert_eq!(roundtrip(&PlayerAbilities { flying: true }), [2]);
        let input = PlayerInput {
            forward: true,
            jump: true,
            sprint: true,
            ..Default::default()
        };
        assert_eq!(roundtrip(&input), [1 | 16 | 64]);
    }

    #[test]
    fn unknown_flag_bits_ignored() {
        let flags = MoveFlags::decode(&mut &[0xfd][..]).unwrap();
        assert_eq!(flags, MoveFlags::new(true, false));
    }
}
