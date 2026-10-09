use mcr_buf::{Decode, Encode, Result};

#[derive(Debug, Clone, Copy, PartialEq, Default, Encode, Decode)]
pub struct Vec3d {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Encode, Decode)]
pub struct Vec3f {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// Packed into one i64 on the wire: x in the low 32 bits, z in the high 32.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ChunkPos {
    pub x: i32,
    pub z: i32,
}

impl Encode for ChunkPos {
    fn encode(&self, out: &mut Vec<u8>) {
        ((self.x as u32 as u64) | ((self.z as u32 as u64) << 32)).encode(out);
    }
}

impl Decode<'_> for ChunkPos {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        let v = u64::decode(input)?;
        Ok(Self {
            x: v as u32 as i32,
            z: (v >> 32) as u32 as i32,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::roundtrip;

    #[test]
    fn chunk_pos_packing() {
        let bytes = roundtrip(&ChunkPos { x: -1, z: 2 });
        assert_eq!(bytes, [0, 0, 0, 2, 0xff, 0xff, 0xff, 0xff]);
    }
}
