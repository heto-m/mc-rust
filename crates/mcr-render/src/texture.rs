use crate::animation::SpriteAnimation;

/// One size class of block textures: every layer shares `width x height`, and each mip level holds all
/// layers back to back as RGBA8 in gamma space.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextureGroupData {
    pub width: u32,
    pub height: u32,
    pub layers: u32,
    pub mips: Vec<Vec<u8>>,
    /// Animated sprites whose first layer lives in this group.
    pub animations: Vec<SpriteAnimation>,
}

pub const MAX_TEXTURE_GROUPS: usize = 4;

impl TextureGroupData {
    pub fn mip_size(&self, level: usize) -> (u32, u32) {
        ((self.width >> level).max(1), (self.height >> level).max(1))
    }

    pub fn single_pixel() -> Self {
        Self {
            width: 1,
            height: 1,
            layers: 1,
            mips: vec![vec![255; 4]],
            animations: Vec::new(),
        }
    }
}

/// RGBA8 base-level block textures; the renderer builds the mip chain.
#[derive(Debug, Clone)]
pub struct TextureArrayData {
    pub size: u32,
    pub layers: Vec<Vec<u8>>,
}

impl TextureArrayData {
    pub fn mip_levels(&self) -> u32 {
        self.size.ilog2() + 1
    }

    pub fn into_group(self) -> TextureGroupData {
        let mut mips = vec![self.layers.concat()];
        let (mut size, count) = (self.size, self.layers.len());
        for _ in 1..self.mip_levels() {
            let prev = mips.last().expect("base level");
            let layer_len = (size * size * 4) as usize;
            let next: Vec<u8> = prev
                .chunks_exact(layer_len)
                .flat_map(|l| downsample(l, size))
                .collect();
            size = (size / 2).max(1);
            mips.push(next);
        }
        TextureGroupData {
            width: self.size,
            height: self.size,
            layers: count as u32,
            mips,
            animations: Vec::new(),
        }
    }

    /// Procedural stand-in: noisy tinted tiles with darker borders.
    pub fn placeholder(layer_count: u32) -> Self {
        const SIZE: u32 = 16;
        const TINTS: [[u8; 3]; 8] = [
            [96, 168, 64],
            [120, 150, 70],
            [134, 96, 67],
            [128, 128, 132],
            [219, 207, 142],
            [240, 245, 250],
            [190, 90, 60],
            [90, 120, 200],
        ];
        let layers = (0..layer_count)
            .map(|l| {
                let tint = TINTS[l as usize % TINTS.len()];
                let mut px = Vec::with_capacity((SIZE * SIZE * 4) as usize);
                for y in 0..SIZE {
                    for x in 0..SIZE {
                        let h = hash(x, y, l);
                        let n = 0.8 + 0.2 * (h & 0xff) as f32 / 255.0;
                        let edge = if x == 0 || y == 0 || x == SIZE - 1 || y == SIZE - 1 {
                            0.8
                        } else {
                            1.0
                        };
                        let c = tint.map(|t| (f32::from(t) * n * edge) as u8);
                        px.extend_from_slice(&[c[0], c[1], c[2], 255]);
                    }
                }
                px
            })
            .collect();
        Self { size: SIZE, layers }
    }
}

fn hash(x: u32, y: u32, z: u32) -> u32 {
    let mut h =
        x.wrapping_mul(0x9e37_79b1) ^ y.wrapping_mul(0x85eb_ca6b) ^ z.wrapping_mul(0xc2b2_ae35);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^ h >> 12
}

pub(crate) fn downsample(src: &[u8], size: u32) -> Vec<u8> {
    let half = (size / 2).max(1);
    let mut out = Vec::with_capacity((half * half * 4) as usize);
    let at = |x: u32, y: u32, c: usize| {
        u32::from(src[((y.min(size - 1) * size + x.min(size - 1)) * 4) as usize + c])
    };
    for y in 0..half {
        for x in 0..half {
            for c in 0..4 {
                let sum = at(2 * x, 2 * y, c)
                    + at(2 * x + 1, 2 * y, c)
                    + at(2 * x, 2 * y + 1, c)
                    + at(2 * x + 1, 2 * y + 1, c);
                out.push(((sum + 2) / 4) as u8);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mip_chain_reaches_one_pixel() {
        let t = TextureArrayData::placeholder(2);
        assert_eq!(t.mip_levels(), 5);
        let mut level = t.layers[0].clone();
        let mut size = t.size;
        while size > 1 {
            level = downsample(&level, size);
            size /= 2;
        }
        assert_eq!(level.len(), 4);
        assert_eq!(level[3], 255);
    }
}
