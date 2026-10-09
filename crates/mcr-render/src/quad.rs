//! Packed quad formats. The WGSL decoders in `shaders/quad.wgsl` mirror these layouts exactly.
//!
//! Cube quad, 8 bytes (`[w0, w1]`):
//! - `w0`: x `[0,5)`, y `[5,10)`, z `[10,15)` block coords inside the section, face `[15,18)`,
//!   width-1 `[18,22)`, height-1 `[22,26)`, tint kind `[26,32)`.
//! - `w1`: texture layer `[0,16)`, AO `[16,24)` as four 2-bit corners, light `[24,32)` (sky<<4 | block).
//!
//! Model quad, 32 bytes (eight `u32`):
//! - `w0..w3`: corner positions, `x[0,10) y[10,20) z[20,30)` in 1/32 block with a -1 block bias.
//! - `w4,w5`: corner UVs, 16 bits each (`u[0,8) v[8,16)`, 1/16 tile units), corners 0,1 and 2,3.
//! - `w6`: layer `[0,16)`, light `[16,24)`, shading face `[24,27)`, flags `[27,32)`.
//! - `w7`: AO as four 2-bit corners `[0,8)`, tint kind `[8,14)`, tint cell x `[14,18)` and z `[18,22)`.
//!
//! Corners are ordered (0,0), (1,0), (0,1), (1,1) in (u, v).

use bytemuck::{Pod, Zeroable};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Face {
    PosX = 0,
    NegX = 1,
    PosY = 2,
    NegY = 3,
    PosZ = 4,
    NegZ = 5,
}

impl Face {
    pub const ALL: [Face; 6] = [
        Face::PosX,
        Face::NegX,
        Face::PosY,
        Face::NegY,
        Face::PosZ,
        Face::NegZ,
    ];

    pub fn from_index(i: u8) -> Option<Face> {
        Self::ALL.get(i as usize).copied()
    }

    pub fn normal(self) -> [i32; 3] {
        match self {
            Face::PosX => [1, 0, 0],
            Face::NegX => [-1, 0, 0],
            Face::PosY => [0, 1, 0],
            Face::NegY => [0, -1, 0],
            Face::PosZ => [0, 0, 1],
            Face::NegZ => [0, 0, -1],
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum QuadError {
    #[error("{field} value {value} out of range")]
    OutOfRange { field: &'static str, value: f64 },
}

fn check(field: &'static str, value: u32, max: u32) -> Result<u32, QuadError> {
    if value > max {
        return Err(QuadError::OutOfRange {
            field,
            value: f64::from(value),
        });
    }
    Ok(value)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CubeQuad {
    pub x: u8,
    pub y: u8,
    pub z: u8,
    pub face: Face,
    pub width: u8,
    pub height: u8,
    pub layer: u16,
    pub ao: [u8; 4],
    pub light: u8,
    pub tint: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Pod, Zeroable)]
#[repr(C)]
pub struct PackedQuad(pub [u32; 2]);

impl PackedQuad {
    pub const SIZE: usize = 8;

    pub fn encode(q: &CubeQuad) -> Result<Self, QuadError> {
        let x = check("x", q.x.into(), 31)?;
        let y = check("y", q.y.into(), 31)?;
        let z = check("z", q.z.into(), 31)?;
        if !(1..=16).contains(&q.width) {
            return Err(QuadError::OutOfRange {
                field: "width",
                value: q.width.into(),
            });
        }
        if !(1..=16).contains(&q.height) {
            return Err(QuadError::OutOfRange {
                field: "height",
                value: q.height.into(),
            });
        }
        let mut ao = 0u32;
        for (i, a) in q.ao.iter().enumerate() {
            ao |= check("ao", (*a).into(), 3)? << (2 * i);
        }
        let w0 = x
            | y << 5
            | z << 10
            | (q.face as u32) << 15
            | u32::from(q.width - 1) << 18
            | u32::from(q.height - 1) << 22
            | check("tint", q.tint.into(), 63)? << 26;
        let w1 = u32::from(q.layer) | ao << 16 | u32::from(q.light) << 24;
        Ok(Self([w0, w1]))
    }

    pub fn decode(self) -> CubeQuad {
        let [w0, w1] = self.0;
        CubeQuad {
            x: (w0 & 31) as u8,
            y: (w0 >> 5 & 31) as u8,
            z: (w0 >> 10 & 31) as u8,
            face: Face::ALL[(w0 >> 15 & 7).min(5) as usize],
            width: (w0 >> 18 & 15) as u8 + 1,
            height: (w0 >> 22 & 15) as u8 + 1,
            layer: (w1 & 0xffff) as u16,
            ao: std::array::from_fn(|i| (w1 >> (16 + 2 * i) & 3) as u8),
            light: (w1 >> 24) as u8,
            tint: (w0 >> 26) as u8,
        }
    }

    pub fn face(self) -> Face {
        Face::ALL[(self.0[0] >> 15 & 7).min(5) as usize]
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelQuadDesc {
    pub corners: [[f32; 3]; 4],
    pub uvs: [[f32; 2]; 4],
    pub layer: u16,
    pub light: u8,
    pub shade_face: Face,
    pub flags: u8,
    pub ao: [u8; 4],
    pub tint: u8,
    pub cell: [u8; 2],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Pod, Zeroable)]
#[repr(C)]
pub struct ModelQuad(pub [u32; 8]);

const POS_SCALE: f32 = 32.0;
const UV_SCALE: f32 = 16.0;

fn quantize(
    field: &'static str,
    v: f32,
    scale: f32,
    bias: f32,
    max: u32,
) -> Result<u32, QuadError> {
    let q = ((v + bias) * scale).round();
    if !(0.0..=max as f32).contains(&q) {
        return Err(QuadError::OutOfRange {
            field,
            value: f64::from(v),
        });
    }
    Ok(q as u32)
}

impl ModelQuad {
    pub const SIZE: usize = 32;

    pub fn encode(q: &ModelQuadDesc) -> Result<Self, QuadError> {
        let mut w = [0u32; 8];
        for (i, c) in q.corners.iter().enumerate() {
            w[i] = quantize("corner x", c[0], POS_SCALE, 1.0, 1023)?
                | quantize("corner y", c[1], POS_SCALE, 1.0, 1023)? << 10
                | quantize("corner z", c[2], POS_SCALE, 1.0, 1023)? << 20;
        }
        for pair in 0..2 {
            for half in 0..2 {
                let uv = q.uvs[pair * 2 + half];
                let packed = quantize("u", uv[0], UV_SCALE, 0.0, 255)?
                    | quantize("v", uv[1], UV_SCALE, 0.0, 255)? << 8;
                w[4 + pair] |= packed << (16 * half);
            }
        }
        w[6] = u32::from(q.layer)
            | u32::from(q.light) << 16
            | (q.shade_face as u32) << 24
            | check("flags", q.flags.into(), 31)? << 27;
        for (i, a) in q.ao.iter().enumerate() {
            w[7] |= check("ao", (*a).into(), 3)? << (2 * i);
        }
        w[7] |= check("tint", q.tint.into(), 63)? << 8
            | check("cell x", q.cell[0].into(), 15)? << 14
            | check("cell z", q.cell[1].into(), 15)? << 18;
        Ok(Self(w))
    }

    pub fn decode(self) -> ModelQuadDesc {
        let w = self.0;
        let unq = |bits: u32| bits as f32 / POS_SCALE - 1.0;
        ModelQuadDesc {
            corners: std::array::from_fn(|i| {
                [
                    unq(w[i] & 1023),
                    unq(w[i] >> 10 & 1023),
                    unq(w[i] >> 20 & 1023),
                ]
            }),
            uvs: std::array::from_fn(|i| {
                let p = w[4 + i / 2] >> (16 * (i % 2));
                [
                    (p & 255) as f32 / UV_SCALE,
                    (p >> 8 & 255) as f32 / UV_SCALE,
                ]
            }),
            layer: (w[6] & 0xffff) as u16,
            light: (w[6] >> 16) as u8,
            shade_face: Face::ALL[(w[6] >> 24 & 7).min(5) as usize],
            flags: (w[6] >> 27) as u8,
            ao: std::array::from_fn(|i| (w[7] >> (2 * i) & 3) as u8),
            tint: (w[7] >> 8 & 63) as u8,
            cell: [(w[7] >> 14 & 15) as u8, (w[7] >> 18 & 15) as u8],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube(x: u8, face: Face) -> CubeQuad {
        CubeQuad {
            x,
            y: 31,
            z: 1,
            face,
            width: 16,
            height: 1,
            layer: 0xbeef,
            ao: [0, 1, 2, 3],
            light: 0xa5,
            tint: 63,
        }
    }

    #[test]
    fn cube_round_trip() {
        for face in Face::ALL {
            for x in [0, 15, 31] {
                let q = cube(x, face);
                assert_eq!(PackedQuad::encode(&q).unwrap().decode(), q);
            }
        }
    }

    #[test]
    fn cube_rejects_out_of_range() {
        let mut q = cube(0, Face::PosX);
        q.x = 32;
        assert!(PackedQuad::encode(&q).is_err());
        let mut q = cube(0, Face::PosX);
        q.width = 17;
        assert!(PackedQuad::encode(&q).is_err());
        let mut q = cube(0, Face::PosX);
        q.height = 0;
        assert!(PackedQuad::encode(&q).is_err());
        let mut q = cube(0, Face::PosX);
        q.ao[2] = 4;
        assert!(PackedQuad::encode(&q).is_err());
    }

    pub(crate) fn sample_model() -> ModelQuadDesc {
        ModelQuadDesc {
            corners: [
                [-1.0, 0.0, 0.5],
                [0.03125, 15.5, 30.96875],
                [30.96875, 8.0, 0.0],
                [16.0, 16.0, 16.0],
            ],
            uvs: [[0.0, 0.0], [15.9375, 0.0], [0.0, 1.5], [8.0, 15.9375]],
            layer: 65535,
            light: 0x3c,
            shade_face: Face::NegZ,
            flags: 17,
            ao: [3, 2, 1, 0],
            tint: 9,
            cell: [15, 3],
        }
    }

    #[test]
    fn model_round_trip() {
        let q = sample_model();
        assert_eq!(ModelQuad::encode(&q).unwrap().decode(), q);
    }

    #[test]
    fn model_rejects_out_of_range() {
        let mut q = sample_model();
        q.corners[0][1] = 32.0;
        assert!(ModelQuad::encode(&q).is_err());
        let mut q = sample_model();
        q.corners[0][0] = -1.5;
        assert!(ModelQuad::encode(&q).is_err());
        let mut q = sample_model();
        q.uvs[1][0] = 16.0;
        assert!(ModelQuad::encode(&q).is_err());
    }

    #[test]
    fn sizes() {
        assert_eq!(size_of::<PackedQuad>(), PackedQuad::SIZE);
        assert_eq!(size_of::<ModelQuad>(), ModelQuad::SIZE);
    }
}
