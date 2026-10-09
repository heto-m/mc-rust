use mcr_buf::{encode_str, write_varint};

pub const PROTOCOL: i32 = 777;
const INTENT_LOGIN: i32 = 2;

pub fn intention(host: &str, port: u16) -> Vec<u8> {
    let mut out = Vec::new();
    write_varint(&mut out, PROTOCOL);
    encode_str(host, &mut out);
    out.extend_from_slice(&port.to_be_bytes());
    write_varint(&mut out, INTENT_LOGIN);
    out
}

pub fn hello(name: &str, uuid: &[u8; 16]) -> Vec<u8> {
    let mut out = Vec::new();
    encode_str(name, &mut out);
    out.extend_from_slice(uuid);
    out
}

pub fn client_information(view_distance: i8) -> Vec<u8> {
    let mut out = Vec::new();
    encode_str("en_us", &mut out);
    out.push(view_distance as u8);
    write_varint(&mut out, 0);
    out.push(1);
    out.push(0x7f);
    write_varint(&mut out, 1);
    out.push(0);
    out.push(1);
    write_varint(&mut out, 0);
    out
}

pub fn empty_known_packs() -> Vec<u8> {
    vec![0]
}

pub fn accept_teleportation(id: i32, pos: [f64; 3], rot: [f32; 2]) -> Vec<u8> {
    let mut out = Vec::new();
    write_varint(&mut out, id);
    for v in pos {
        out.extend_from_slice(&v.to_be_bytes());
    }
    for v in rot {
        out.extend_from_slice(&v.to_be_bytes());
    }
    out
}

pub fn chunk_batch_received(chunks_per_tick: f32) -> Vec<u8> {
    chunks_per_tick.to_be_bytes().to_vec()
}

pub fn move_player_pos_rot(pos: [f64; 3], rot: [f32; 2], on_ground: bool) -> Vec<u8> {
    let mut out = Vec::new();
    for v in pos {
        out.extend_from_slice(&v.to_be_bytes());
    }
    for v in rot {
        out.extend_from_slice(&v.to_be_bytes());
    }
    out.push(u8::from(on_ground));
    out
}

pub struct Teleport {
    pub id: i32,
    pub pos: [f64; 3],
    pub rot: [f32; 2],
}

const POSITION_ROTATION_OFFSET: usize = 48;

pub fn parse_player_position(mut payload: &[u8]) -> Option<Teleport> {
    let id = mcr_buf::read_varint(&mut payload).ok()?;
    let f64_at = |i: usize| -> Option<f64> {
        Some(f64::from_be_bytes(payload.get(i..i + 8)?.try_into().ok()?))
    };
    let f32_at = |i: usize| -> Option<f32> {
        Some(f32::from_be_bytes(payload.get(i..i + 4)?.try_into().ok()?))
    };
    Some(Teleport {
        id,
        pos: [f64_at(0)?, f64_at(8)?, f64_at(16)?],
        rot: [
            f32_at(POSITION_ROTATION_OFFSET)?,
            f32_at(POSITION_ROTATION_OFFSET + 4)?,
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intention_layout() {
        let b = intention("localhost", 25565);
        assert_eq!(&b[..3], &[0x89, 0x06, 9]);
        assert_eq!(&b[3 + 9..], &[0x63, 0xdd, 2]);
    }

    #[test]
    fn hello_layout() {
        let b = hello("Cap", &[7; 16]);
        assert_eq!(b.len(), 1 + 3 + 16);
        assert_eq!(&b[..4], &[3, b'C', b'a', b'p']);
    }

    #[test]
    fn client_information_layout() {
        let b = client_information(8);
        assert_eq!(b.len(), 6 + 8);
        assert_eq!(b[6], 8);
    }

    #[test]
    fn teleport_roundtrip() {
        let mut p = Vec::new();
        write_varint(&mut p, 5);
        for v in [1.0f64, 2.0, 3.0, 0.0, 0.0, 0.0] {
            p.extend_from_slice(&v.to_be_bytes());
        }
        for v in [90.0f32, 10.0] {
            p.extend_from_slice(&v.to_be_bytes());
        }
        p.extend_from_slice(&[0; 4]);
        let t = parse_player_position(&p).unwrap();
        assert_eq!((t.id, t.pos, t.rot), (5, [1.0, 2.0, 3.0], [90.0, 10.0]));
    }

    #[test]
    fn move_layout() {
        assert_eq!(
            move_player_pos_rot([0.0; 3], [0.0; 2], true).len(),
            24 + 8 + 1
        );
    }
}
