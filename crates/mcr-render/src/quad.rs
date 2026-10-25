//! Packed quad formats. The WGSL decoders in `shaders/quad.wgsl` mirror these layouts exactly.
//!
//! Light is `sky << 8 | block` with each half in 1/16 light levels (0..=240), the smooth-lighting scale.
//!
//! Cube quad, 16 bytes (`[w0, w1, w2, w3]`):
//! - `w0`: x `[0,5)`, y `[5,10)`, z `[10,15)` block coords inside the section, face `[15,18)`,
//!   width-1 `[18,22)`, height-1 `[22,26)`, tint kind `[26,32)`.
//! - `w1`: texture layer `[0,16)`, AO `[16,28)` as four 3-bit levels (0..=4, brightness `0.2 + 0.2 * level`),
//!   UV orientation `[28,31)`: quarter turns in the low two bits, bit 2 mirrors u first; bit 31 draws the
//!   texture without alpha cutout.
//! - `w2`, `w3`: corner light, corners 0,1 and 2,3, 16 bits each.
//!
//! Model quad, 96 bytes (24 `u32`):
//! - `w0..w11`: corner positions as section-local `f32`, x y z per corner, the values vanilla computes.
//! - `w12..w19`: corner UVs as `f32` tile units, u v per corner.
//! - `w20`: layer `[0,16)`, shading face `[16,19)`, flags `[19,22)` (`MODEL_FLAG_*`), tint kind `[22,28)`,
//!   base cell y `[28,32)`.
//! - `w21,w22`: corner light, corners 0,1 and 2,3, 16 bits each.
//! - `w23`: AO as four 6-bit brightness values (`v / 63`) `[0,24)`, base cell x `[24,28)`, z `[28,32)`.
//!
//! Cube corners are ordered (0,0), (1,0), (0,1), (1,1) in (u, v). Model corners are in drawing order: the
//! shader emits triangles `(0, 1, 2)` and `(2, 1, 3)`.

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

/// Packs smooth light levels, each in 1/16 light levels (0..=240).
pub const fn pack_light(sky: u8, block: u8) -> u16 {
    (sky as u16) << 8 | block as u16
}

pub const fn light_sky(light: u16) -> u8 {
    (light >> 8) as u8
}

pub const fn light_block(light: u16) -> u8 {
    light as u8
}

/// Model quad flag: the texture is drawn without alpha cutout.
pub const MODEL_FLAG_OPAQUE: u8 = 1;
/// Model quad flag: also shaded by the up face, like vanilla's fluid sides.
pub const MODEL_FLAG_UP_SHADE: u8 = 2;

/// Full daylight and no block light.
pub const LIGHT_FULL_SKY: u16 = pack_light(240, 0);

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
    pub light: [u16; 4],
    pub tint: u8,
    pub rotation: u8,
    pub opaque: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Pod, Zeroable)]
#[repr(C)]
pub struct PackedQuad(pub [u32; 4]);

impl PackedQuad {
    pub const SIZE: usize = 16;

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
            ao |= check("ao", (*a).into(), 4)? << (3 * i);
        }
        let w0 = x
            | y << 5
            | z << 10
            | (q.face as u32) << 15
            | u32::from(q.width - 1) << 18
            | u32::from(q.height - 1) << 22
            | check("tint", q.tint.into(), 63)? << 26;
        let w1 = u32::from(q.layer)
            | ao << 16
            | check("rotation", q.rotation.into(), 7)? << 28
            | u32::from(q.opaque) << 31;
        let l = |i: usize| u32::from(q.light[i]);
        Ok(Self([w0, w1, l(0) | l(1) << 16, l(2) | l(3) << 16]))
    }

    pub fn decode(self) -> CubeQuad {
        let [w0, w1, w2, w3] = self.0;
        CubeQuad {
            x: (w0 & 31) as u8,
            y: (w0 >> 5 & 31) as u8,
            z: (w0 >> 10 & 31) as u8,
            face: Face::ALL[(w0 >> 15 & 7).min(5) as usize],
            width: (w0 >> 18 & 15) as u8 + 1,
            height: (w0 >> 22 & 15) as u8 + 1,
            layer: (w1 & 0xffff) as u16,
            ao: std::array::from_fn(|i| (w1 >> (16 + 3 * i) & 7) as u8),
            light: [w2 as u16, (w2 >> 16) as u16, w3 as u16, (w3 >> 16) as u16],
            tint: (w0 >> 26) as u8,
            rotation: (w1 >> 28 & 7) as u8,
            opaque: w1 >> 31 != 0,
        }
    }

    pub fn face(self) -> Face {
        Face::ALL[(self.0[0] >> 15 & 7).min(5) as usize]
    }

    pub fn is_opaque(self) -> bool {
        self.0[1] >> 31 != 0
    }
}

/// Brightness of a cube AO level.
pub fn cube_ao_brightness(level: u8) -> f32 {
    0.2 + 0.2 * f32::from(level)
}

/// Quantises a 0..=1 brightness to the model quad's six-bit AO.
pub fn model_ao(brightness: f32) -> u8 {
    (brightness.clamp(0.0, 1.0) * 63.0).round() as u8
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelQuadDesc {
    /// Section-local positions, each within `-1..3` blocks of `cell`.
    pub corners: [[f32; 3]; 4],
    pub uvs: [[f32; 2]; 4],
    pub layer: u16,
    pub light: [u16; 4],
    pub shade_face: Face,
    pub flags: u8,
    /// Six-bit brightness per corner (`model_ao`).
    pub ao: [u8; 4],
    pub tint: u8,
    /// Base block cell inside the section; also the tint column.
    pub cell: [u8; 3],
}

pub const MODEL_QUAD_WORDS: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Pod, Zeroable)]
#[repr(C)]
pub struct ModelQuad(pub [u32; MODEL_QUAD_WORDS]);

/// Merged fluid tops repeat their sprite a few times; nothing else leaves the tile.
const MAX_UV: f32 = 16.0;

impl ModelQuad {
    pub const SIZE: usize = MODEL_QUAD_WORDS * 4;

    pub fn is_opaque(&self) -> bool {
        self.0[20] >> 19 & u32::from(MODEL_FLAG_OPAQUE) != 0
    }

    pub fn encode(q: &ModelQuadDesc) -> Result<Self, QuadError> {
        let mut w = [0u32; MODEL_QUAD_WORDS];
        for cell in q.cell {
            check("cell", cell.into(), 15)?;
        }
        for (i, c) in q.corners.iter().enumerate() {
            for (axis, &v) in c.iter().enumerate() {
                if !(-1.0..=3.0).contains(&(v - f32::from(q.cell[axis]))) {
                    return Err(QuadError::OutOfRange {
                        field: "corner",
                        value: f64::from(v),
                    });
                }
                w[i * 3 + axis] = v.to_bits();
            }
        }
        for (i, uv) in q.uvs.iter().enumerate() {
            for (k, &v) in uv.iter().enumerate() {
                if !(0.0..=MAX_UV).contains(&v) {
                    return Err(QuadError::OutOfRange {
                        field: "uv",
                        value: f64::from(v),
                    });
                }
                w[12 + i * 2 + k] = v.to_bits();
            }
        }
        w[20] = u32::from(q.layer)
            | (q.shade_face as u32) << 16
            | check("flags", q.flags.into(), 7)? << 19
            | check("tint", q.tint.into(), 63)? << 22
            | u32::from(q.cell[1]) << 28;
        for pair in 0..2 {
            w[21 + pair] = u32::from(q.light[pair * 2]) | u32::from(q.light[pair * 2 + 1]) << 16;
        }
        for (i, a) in q.ao.iter().enumerate() {
            w[23] |= check("ao", (*a).into(), 63)? << (6 * i);
        }
        w[23] |= u32::from(q.cell[0]) << 24 | u32::from(q.cell[2]) << 28;
        Ok(Self(w))
    }

    pub fn decode(self) -> ModelQuadDesc {
        let w = self.0;
        ModelQuadDesc {
            corners: std::array::from_fn(|i| std::array::from_fn(|a| f32::from_bits(w[i * 3 + a]))),
            uvs: std::array::from_fn(|i| {
                std::array::from_fn(|k| f32::from_bits(w[12 + i * 2 + k]))
            }),
            layer: (w[20] & 0xffff) as u16,
            light: std::array::from_fn(|i| (w[21 + i / 2] >> (16 * (i % 2))) as u16),
            shade_face: Face::ALL[(w[20] >> 16 & 7).min(5) as usize],
            flags: (w[20] >> 19 & 7) as u8,
            ao: std::array::from_fn(|i| (w[23] >> (6 * i) & 63) as u8),
            tint: (w[20] >> 22 & 63) as u8,
            cell: [
                (w[23] >> 24 & 15) as u8,
                (w[20] >> 28) as u8,
                (w[23] >> 28) as u8,
            ],
        }
    }
}

/// Section-local centre of a model quad, the point vanilla's translucent sort measures from.
pub fn model_centroid(q: &ModelQuad) -> [f32; 3] {
    std::array::from_fn(|a| (0..4).map(|c| f32::from_bits(q.0[c * 3 + a])).sum::<f32>() / 4.0)
}

/// Orders translucent quads back to front from `eye` (section-local), like vanilla's
/// `VertexSorting.byDistance`: farthest centroid first, ties in their original order.
pub fn sort_back_to_front(quads: &mut [ModelQuad], eye: [f32; 3], scratch: &mut Vec<u64>) {
    scratch.clear();
    scratch.extend(quads.iter().enumerate().map(|(i, q)| {
        let c = model_centroid(q);
        let d = (0..3)
            .map(|a| (c[a] - eye[a]) * (c[a] - eye[a]))
            .sum::<f32>();
        u64::from(!d.to_bits()) << 32 | i as u64
    }));
    scratch.sort_unstable();
    let original = quads.to_vec();
    for (slot, key) in quads.iter_mut().zip(scratch.iter()) {
        *slot = original[(*key & 0xffff_ffff) as usize];
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
            ao: [0, 1, 4, 3],
            light: [0xf000, 0x00f0, 0x1234, 0xffff],
            tint: 63,
            rotation: 7,
            opaque: true,
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
        q.ao[2] = 5;
        assert!(PackedQuad::encode(&q).is_err());
        let mut q = cube(0, Face::PosX);
        q.rotation = 8;
        assert!(PackedQuad::encode(&q).is_err());
    }

    pub(crate) fn sample_model() -> ModelQuadDesc {
        ModelQuadDesc {
            corners: [
                [14.0, 3.0, 3.5],
                [15.0078125, 5.5, 5.9921875],
                [14.0, 3.0, 3.0],
                [15.0, 5.0, 5.0],
            ],
            uvs: [[0.0, 0.0], [15.9375, 0.0], [0.0, 1.5], [8.0, 15.9375]],
            layer: 65535,
            light: [0x3c3c, 0xf000, 0x00f0, 0x0102],
            shade_face: Face::NegZ,
            flags: 5,
            ao: [63, 32, 1, 0],
            tint: 9,
            cell: [15, 3, 4],
        }
    }

    #[test]
    fn model_round_trip() {
        let q = sample_model();
        assert_eq!(ModelQuad::encode(&q).unwrap().decode(), q);
    }

    #[test]
    fn model_positions_and_uvs_keep_every_bit() {
        let mut q = sample_model();
        q.corners[0] = [14.999, 3.887_889, 3.0703125];
        q.uvs[2] = [0.129_9, 0.853_553_4];
        assert_eq!(ModelQuad::encode(&q).unwrap().decode(), q);
    }

    #[test]
    fn model_rejects_out_of_range() {
        let mut q = sample_model();
        q.corners[0][1] = 8.0;
        assert!(ModelQuad::encode(&q).is_err());
        let mut q = sample_model();
        q.corners[0][0] = 12.9;
        assert!(ModelQuad::encode(&q).is_err());
        let mut q = sample_model();
        q.uvs[1][0] = -0.25;
        assert!(ModelQuad::encode(&q).is_err());
        let mut q = sample_model();
        q.uvs[1][0] = f32::NAN;
        assert!(ModelQuad::encode(&q).is_err());
        let mut q = sample_model();
        q.corners[3][2] = f32::INFINITY;
        assert!(ModelQuad::encode(&q).is_err());
        let mut q = sample_model();
        q.ao[0] = 64;
        assert!(ModelQuad::encode(&q).is_err());
    }

    #[test]
    fn centroid_is_the_mean_corner() {
        let q = ModelQuad::encode(&sample_model()).unwrap();
        let c = model_centroid(&q);
        let corners = q.decode().corners;
        for a in 0..3 {
            let mean = corners.iter().map(|p| p[a]).sum::<f32>() / 4.0;
            assert!((c[a] - mean).abs() < 1e-4, "{a}: {} vs {mean}", c[a]);
        }
    }

    #[test]
    fn sort_puts_the_farthest_quad_first_and_keeps_ties() {
        let at = |x: f32, layer: u16| {
            let mut d = sample_model();
            d.cell = [x as u8, 0, 0];
            d.corners = [
                [x, 0.0, 0.0],
                [x + 1.0, 0.0, 0.0],
                [x, 1.0, 0.0],
                [x + 1.0, 1.0, 0.0],
            ];
            d.layer = layer;
            ModelQuad::encode(&d).unwrap()
        };
        let mut quads = [at(2.0, 1), at(9.0, 2), at(5.0, 3), at(9.0, 4)];
        sort_back_to_front(&mut quads, [0.0, 0.5, 0.0], &mut Vec::new());
        let order: Vec<u16> = quads.iter().map(|q| q.decode().layer).collect();
        assert_eq!(order, [2, 4, 3, 1]);
    }

    #[test]
    fn sizes() {
        assert_eq!(size_of::<PackedQuad>(), PackedQuad::SIZE);
        assert_eq!(size_of::<ModelQuad>(), ModelQuad::SIZE);
    }
}
