//! Padded mesher input: a section's 16^3 states and light plus a one-block border.

use mcr_world::{ChunkColumn, ChunkPos, ChunkStore, LightSlot, SectionPos, block_index};

use crate::tint::MAX_BLEND_RADIUS;

pub const PAD: usize = 18;
pub const PADDED: usize = PAD * PAD * PAD;

/// Index of the cell at section-local `(x, y, z)`, each in `-1..=16`.
#[inline]
pub const fn pidx(x: i32, y: i32, z: i32) -> usize {
    (((y + 1) * PAD as i32 + (z + 1)) * PAD as i32 + (x + 1)) as usize
}

/// Light assumed where a light section has not arrived yet: daylight, no block light.
pub const UNLIT_LIGHT: u8 = 0xf0;

#[derive(Debug, Clone)]
pub struct MeshInput {
    pub pos: SectionPos,
    pub states: Box<[u16; PADDED]>,
    /// `sky << 4 | block`.
    pub light: Box<[u8; PADDED]>,
    /// Biome ids on the quart grid around the section: x and z quarts `-3..=6`, y quarts `-1..=4`,
    /// indexed `(qy * 10 + qz) * 10 + qx` with the origin offsets applied (`quart_index`).
    pub biomes: Vec<u16>,
    pub blend_radius: usize,
    /// The hashed world seed vanilla's biome zoom is keyed with.
    pub biome_seed: i64,
}

pub const QUART_SPAN: usize = 10;
pub const QUART_SPAN_Y: usize = 6;
pub const QUART_ORIGIN_XZ: i32 = -3;
pub const QUART_ORIGIN_Y: i32 = -1;

/// Index of the quart `(qx, qy, qz)` relative to the section's first quart, clamped to the grid.
pub fn quart_index(qx: i32, qy: i32, qz: i32) -> usize {
    let x = (qx - QUART_ORIGIN_XZ).clamp(0, QUART_SPAN as i32 - 1) as usize;
    let y = (qy - QUART_ORIGIN_Y).clamp(0, QUART_SPAN_Y as i32 - 1) as usize;
    let z = (qz - QUART_ORIGIN_XZ).clamp(0, QUART_SPAN as i32 - 1) as usize;
    (y * QUART_SPAN + z) * QUART_SPAN + x
}

impl MeshInput {
    pub fn blank(pos: SectionPos, blend_radius: usize) -> Self {
        let radius = blend_radius.min(MAX_BLEND_RADIUS);
        Self {
            pos,
            states: vec![0u16; PADDED]
                .into_boxed_slice()
                .try_into()
                .expect("size"),
            light: vec![UNLIT_LIGHT; PADDED]
                .into_boxed_slice()
                .try_into()
                .expect("size"),
            biomes: vec![0; QUART_SPAN * QUART_SPAN * QUART_SPAN_Y],
            blend_radius: radius,
            biome_seed: 0,
        }
    }

    pub fn set_state(&mut self, x: i32, y: i32, z: i32, state: u16) {
        self.states[pidx(x, y, z)] = state;
    }

    pub fn state(&self, x: i32, y: i32, z: i32) -> u16 {
        self.states[pidx(x, y, z)]
    }

    pub fn set_light(&mut self, x: i32, y: i32, z: i32, sky: u8, block: u8) {
        self.light[pidx(x, y, z)] = sky << 4 | block;
    }

    pub fn is_all_air(&self) -> bool {
        self.states.iter().all(|&s| s == 0)
    }

    /// Copies the section at `pos` and its 26 neighbours out of `store`; `None` when the section is
    /// not loaded.
    pub fn snapshot(store: &ChunkStore, pos: SectionPos, blend_radius: usize) -> Option<Self> {
        let column = store.column(pos.chunk())?;
        let index = usize::try_from(pos.y - (column.min_y() >> 4)).ok()?;
        let center = column.section(index)?;
        let mut input = Self::blank(pos, blend_radius);

        let mut blocks = [0u16; 4096];
        center.unpack_blocks(&mut blocks);
        for y in 0..16 {
            for z in 0..16 {
                let src = y * 256 + z * 16;
                let dst = pidx(0, y as i32, z as i32);
                input.states[dst..dst + 16].copy_from_slice(&blocks[src..src + 16]);
            }
        }
        input.copy_light(column, index, 0, 0, 0);

        for dy in -1..=1i32 {
            for dz in -1..=1i32 {
                for dx in -1..=1i32 {
                    if (dx, dy, dz) == (0, 0, 0) {
                        continue;
                    }
                    input.copy_border(store, pos, dx, dy, dz);
                }
            }
        }
        input.fill_biomes(store, pos);
        Some(input)
    }

    fn copy_light(&mut self, column: &ChunkColumn, index: usize, dx: i32, dy: i32, dz: i32) {
        let slot = (index as i32 + 1 + dy) as usize;
        let (sky, above) = match column.sky_light_section(slot) {
            None | Some(LightSlot::Absent) => (column.sky_light_above(slot), true),
            present => (present, false),
        };
        let block = column.block_light_section(slot);
        let (xr, yr, zr) = (border_range(dx), border_range(dy), border_range(dz));
        for y in yr.clone() {
            for z in zr.clone() {
                for x in xr.clone() {
                    let (lx, ly, lz) = ((x & 15) as usize, (y & 15) as usize, (z & 15) as usize);
                    let s = light_at(sky, lx, if above { 0 } else { ly }, lz, 15);
                    let b = light_at(block, lx, ly, lz, 0);
                    self.light[pidx(x, y, z)] = s << 4 | b;
                }
            }
        }
    }

    fn copy_border(&mut self, store: &ChunkStore, pos: SectionPos, dx: i32, dy: i32, dz: i32) {
        let Some(column) = store.column(ChunkPos::new(pos.x + dx, pos.z + dz)) else {
            return;
        };
        let nbase = column.min_y() >> 4;
        let Ok(index) = usize::try_from(pos.y + dy - nbase) else {
            return;
        };
        if let Some(section) = column.section(index)
            && !section.is_empty()
        {
            for y in border_range(dy) {
                for z in border_range(dz) {
                    for x in border_range(dx) {
                        let s =
                            section.block((x & 15) as usize, (y & 15) as usize, (z & 15) as usize);
                        self.states[pidx(x, y, z)] = s.0;
                    }
                }
            }
        }
        self.copy_light(column, index, dx, dy, dz);
    }

    fn fill_biomes(&mut self, store: &ChunkStore, pos: SectionPos) {
        let mut cache: [Option<[u16; 64]>; 27] = [None; 27];
        for (slot, entry) in cache.iter_mut().enumerate() {
            let (cx, cy, cz) = (
                slot as i32 % 3 - 1,
                slot as i32 / 9 - 1,
                slot as i32 / 3 % 3 - 1,
            );
            *entry = store
                .column(ChunkPos::new(pos.x + cx, pos.z + cz))
                .and_then(|c| {
                    let index = usize::try_from(pos.y + cy - (c.min_y() >> 4)).ok()?;
                    let mut out = [0u16; 64];
                    c.section(index)?.unpack_biomes(&mut out);
                    Some(out)
                });
        }
        let center = cache[13].unwrap_or([0; 64]);
        for qy in 0..QUART_SPAN_Y as i32 {
            for qz in 0..QUART_SPAN as i32 {
                for qx in 0..QUART_SPAN as i32 {
                    let (gx, gy, gz) = (
                        qx + QUART_ORIGIN_XZ,
                        qy + QUART_ORIGIN_Y,
                        qz + QUART_ORIGIN_XZ,
                    );
                    let slot = ((gy.div_euclid(4) + 1) * 9
                        + (gz.div_euclid(4) + 1) * 3
                        + gx.div_euclid(4)
                        + 1) as usize;
                    let table = cache[slot].as_ref().unwrap_or(&center);
                    let (lx, ly, lz) = (
                        gx.rem_euclid(4) as usize,
                        gy.rem_euclid(4) as usize,
                        gz.rem_euclid(4) as usize,
                    );
                    self.biomes
                        [(qy as usize * QUART_SPAN + qz as usize) * QUART_SPAN + qx as usize] =
                        table[ly << 4 | lz << 2 | lx];
                }
            }
        }
    }
}

fn border_range(d: i32) -> std::ops::Range<i32> {
    match d {
        -1 => -1..0,
        1 => 16..17,
        _ => 0..16,
    }
}

fn light_at(slot: Option<&LightSlot>, x: usize, y: usize, z: usize, absent: u8) -> u8 {
    match slot {
        Some(LightSlot::Data(n)) => {
            let i = block_index(x, y, z);
            (n.as_bytes()[i >> 1] >> ((i & 1) * 4)) & 15
        }
        Some(LightSlot::Dark) => 0,
        _ => absent,
    }
}
