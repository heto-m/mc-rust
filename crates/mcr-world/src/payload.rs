use mcr_buf::{read_len, read_varint, take, write_varint};

use crate::column::{BlockEntityData, Heightmap, HeightmapKind, Heightmaps};
use crate::nbt_skip::take_optional_nbt;
use crate::{ChunkSection, Result, WorldConfig, WorldError};

pub const LIGHT_BYTES: usize = 2048;
const MAX_SECTION_BUFFER: usize = 2 * 1024 * 1024;
const MAX_BLOCK_ENTITIES: usize = 1 << 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LightNibbles(Box<[u8; LIGHT_BYTES]>);

impl LightNibbles {
    pub fn new(bytes: Box<[u8; LIGHT_BYTES]>) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; LIGHT_BYTES] {
        &self.0
    }

    /// Same y, z, x order as blocks; even indices use the low nibble.
    #[inline]
    pub fn get(&self, x: usize, y: usize, z: usize) -> u8 {
        let i = crate::block_index(x, y, z);
        (self.0[i >> 1] >> ((i & 1) * 4)) & 15
    }

    pub fn set(&mut self, x: usize, y: usize, z: usize, level: u8) {
        let i = crate::block_index(x, y, z);
        let shift = (i & 1) * 4;
        let byte = &mut self.0[i >> 1];
        *byte = (*byte & !(15 << shift)) | ((level & 15) << shift);
    }
}

impl Default for LightNibbles {
    fn default() -> Self {
        Self(Box::new([0; LIGHT_BYTES]))
    }
}

/// The chunk-data struct of the level-chunk-with-light packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkData {
    pub heightmaps: Heightmaps,
    pub sections: Vec<ChunkSection>,
    pub block_entities: Vec<BlockEntityData>,
}

impl ChunkData {
    pub fn decode(input: &mut &[u8], cfg: &WorldConfig) -> Result<Self> {
        let mut heightmaps = Heightmaps::default();
        for _ in 0..read_len(input, HeightmapKind::ALL.len())? {
            let id = read_varint(input)?;
            let kind = u8::try_from(id)
                .ok()
                .and_then(HeightmapKind::from_id)
                .ok_or(WorldError::Decode(mcr_buf::DecodeError::InvalidValue {
                    what: "heightmap type",
                    value: id as i64,
                }))?;
            let words = read_len(input, 4096)?;
            let raw = take(input, words * 8)?;
            let data = raw
                .as_chunks::<8>()
                .0
                .iter()
                .map(|c| u64::from_be_bytes(*c))
                .collect();
            heightmaps.set(kind, Heightmap::from_raw(data, cfg)?);
        }

        let len = read_len(input, MAX_SECTION_BUFFER)?;
        let mut buffer = take(input, len)?;
        let mut sections = Vec::with_capacity(cfg.section_count());
        for _ in 0..cfg.section_count() {
            sections.push(ChunkSection::decode(&mut buffer, cfg)?);
        }
        if !buffer.is_empty() {
            return Err(WorldError::TrailingBytes(buffer.len()));
        }

        let count = read_len(input, MAX_BLOCK_ENTITIES)?;
        let mut block_entities = Vec::with_capacity(count.min(1024));
        for _ in 0..count {
            let head = take(input, 3)?;
            let type_id = read_varint(input)?;
            let type_id = u32::try_from(type_id).map_err(|_| WorldError::IdOutOfRange {
                id: type_id as u32,
                count: i32::MAX as u32,
            })?;
            block_entities.push(BlockEntityData {
                packed_xz: head[0],
                y: i16::from_be_bytes([head[1], head[2]]),
                type_id,
                nbt: take_optional_nbt(input)?.into(),
            });
        }
        Ok(Self {
            heightmaps,
            sections,
            block_entities,
        })
    }

    pub fn encode(&self, out: &mut Vec<u8>) {
        let present: Vec<_> = HeightmapKind::ALL
            .iter()
            .filter_map(|&k| self.heightmaps.get(k).map(|h| (k, h)))
            .collect();
        write_varint(out, present.len() as i32);
        for (kind, map) in present {
            write_varint(out, kind as i32);
            write_varint(out, map.raw().len() as i32);
            for word in map.raw() {
                out.extend_from_slice(&word.to_be_bytes());
            }
        }

        let mut buffer = Vec::new();
        for section in &self.sections {
            section.encode(&mut buffer);
        }
        write_varint(out, buffer.len() as i32);
        out.extend_from_slice(&buffer);

        write_varint(out, self.block_entities.len() as i32);
        for be in &self.block_entities {
            out.push(be.packed_xz);
            out.extend_from_slice(&be.y.to_be_bytes());
            write_varint(out, be.type_id as i32);
            if be.nbt.is_empty() {
                out.push(0);
            } else {
                out.extend_from_slice(&be.nbt);
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LightUpdate {
    pub section: u16,
    /// `None` means the whole section is dark.
    pub data: Option<LightNibbles>,
}

/// Light-data struct; section indices count up from one section below the dimension.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LightData {
    pub sky: Vec<LightUpdate>,
    pub block: Vec<LightUpdate>,
}

impl LightData {
    pub fn decode(input: &mut &[u8], cfg: &WorldConfig) -> Result<Self> {
        let sections = cfg.light_section_count();
        let sky_mask = read_bitset(input, sections)?;
        let block_mask = read_bitset(input, sections)?;
        let sky_empty = read_bitset(input, sections)?;
        let block_empty = read_bitset(input, sections)?;
        let sky = read_layer(input, &sky_mask, &sky_empty, sections)?;
        let block = read_layer(input, &block_mask, &block_empty, sections)?;
        Ok(Self { sky, block })
    }

    pub fn encode(&self, out: &mut Vec<u8>) {
        let (sky_mask, sky_empty, sky_data) = split_layer(&self.sky);
        let (block_mask, block_empty, block_data) = split_layer(&self.block);
        for mask in [&sky_mask, &block_mask, &sky_empty, &block_empty] {
            write_bitset(out, mask);
        }
        for data in [sky_data, block_data] {
            write_varint(out, data.len() as i32);
            for nibbles in data {
                write_varint(out, LIGHT_BYTES as i32);
                out.extend_from_slice(nibbles.as_bytes());
            }
        }
    }
}

fn read_bitset(input: &mut &[u8], bit_limit: usize) -> Result<Vec<u16>> {
    let len = read_len(input, bit_limit.div_ceil(8))?;
    let bytes = take(input, len)?;
    let mut set = Vec::new();
    for (byte_index, &byte) in bytes.iter().enumerate() {
        for bit in 0..8 {
            if byte >> bit & 1 == 1 {
                let index = byte_index * 8 + bit;
                if index >= bit_limit {
                    return Err(WorldError::BadLength {
                        what: "light section index",
                        expected: bit_limit,
                        got: index,
                    });
                }
                set.push(index as u16);
            }
        }
    }
    Ok(set)
}

fn read_layer(
    input: &mut &[u8],
    mask: &[u16],
    empty: &[u16],
    sections: usize,
) -> Result<Vec<LightUpdate>> {
    let count = read_len(input, sections)?;
    if count != mask.len() {
        return Err(WorldError::BadLength {
            what: "light arrays",
            expected: mask.len(),
            got: count,
        });
    }
    let mut updates = Vec::with_capacity(count + empty.len());
    for &section in mask {
        let len = read_len(input, LIGHT_BYTES)?;
        if len != LIGHT_BYTES {
            return Err(WorldError::BadLength {
                what: "light array",
                expected: LIGHT_BYTES,
                got: len,
            });
        }
        let bytes: Box<[u8; LIGHT_BYTES]> = take(input, LIGHT_BYTES)?
            .to_vec()
            .into_boxed_slice()
            .try_into()
            .expect("2048 bytes");
        updates.push(LightUpdate {
            section,
            data: Some(LightNibbles(bytes)),
        });
    }
    updates.extend(empty.iter().map(|&section| LightUpdate {
        section,
        data: None,
    }));
    Ok(updates)
}

fn split_layer(updates: &[LightUpdate]) -> (Vec<u16>, Vec<u16>, Vec<&LightNibbles>) {
    let mut sorted: Vec<_> = updates.iter().collect();
    sorted.sort_by_key(|u| u.section);
    let mut mask = Vec::new();
    let mut empty = Vec::new();
    let mut data = Vec::new();
    for update in sorted {
        match &update.data {
            Some(nibbles) => {
                mask.push(update.section);
                data.push(nibbles);
            }
            None => empty.push(update.section),
        }
    }
    (mask, empty, data)
}

fn write_bitset(out: &mut Vec<u8>, set: &[u16]) {
    let len = set.iter().max().map_or(0, |&m| m as usize / 8 + 1);
    let mut bytes = vec![0u8; len];
    for &bit in set {
        bytes[bit as usize / 8] |= 1 << (bit % 8);
    }
    write_varint(out, len as i32);
    out.extend_from_slice(&bytes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Biome, BlockState};

    fn cfg() -> WorldConfig {
        WorldConfig::new(35723, 80, -64, 384).unwrap()
    }

    fn sample_data(cfg: &WorldConfig) -> ChunkData {
        let mut sections = Vec::new();
        for s in 0..cfg.section_count() {
            let mut sec = ChunkSection::empty(cfg);
            for i in 0..(s * 37) {
                sec.set_block(
                    i % 16,
                    (i / 16) % 16,
                    (i / 7) % 16,
                    BlockState(i as u32 + 1),
                    cfg,
                );
            }
            sec.set_biome(s % 4, 0, 0, Biome(s as u16));
            sections.push(sec);
        }
        let mut heightmaps = Heightmaps::default();
        heightmaps.set(HeightmapKind::MotionBlocking, Heightmap::empty(cfg));
        heightmaps.set(HeightmapKind::WorldSurface, Heightmap::empty(cfg));
        ChunkData {
            heightmaps,
            sections,
            block_entities: vec![
                BlockEntityData {
                    packed_xz: 0x35,
                    y: -12,
                    type_id: 7,
                    nbt: vec![10, 0].into(),
                },
                BlockEntityData {
                    packed_xz: 0,
                    y: 300,
                    type_id: 1,
                    nbt: Box::default(),
                },
            ],
        }
    }

    #[test]
    fn chunk_data_round_trip() {
        let cfg = cfg();
        let data = sample_data(&cfg);
        let mut bytes = Vec::new();
        data.encode(&mut bytes);
        let mut input = bytes.as_slice();
        assert_eq!(ChunkData::decode(&mut input, &cfg).unwrap(), data);
        assert!(input.is_empty());
    }

    #[test]
    fn chunk_data_wire_layout_has_no_array_prefixes() {
        let cfg = WorldConfig::new(35723, 80, 0, 16).unwrap();
        let data = ChunkData {
            heightmaps: Heightmaps::default(),
            sections: vec![ChunkSection::empty(&cfg)],
            block_entities: vec![],
        };
        let mut bytes = Vec::new();
        data.encode(&mut bytes);
        assert_eq!(bytes, [0, 8, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn rejects_trailing_and_truncated_section_buffer() {
        let cfg = cfg();
        let data = sample_data(&cfg);
        let mut bytes = Vec::new();
        data.encode(&mut bytes);
        for cut in (0..bytes.len()).step_by(97) {
            assert!(ChunkData::decode(&mut &bytes[..cut], &cfg).is_err());
        }
        let short = WorldConfig::new(35723, 80, -64, 368).unwrap();
        assert!(ChunkData::decode(&mut bytes.as_slice(), &short).is_err());
    }

    #[test]
    fn light_round_trip_and_nibble_order() {
        let cfg = cfg();
        let mut nib = LightNibbles::default();
        nib.set(1, 0, 0, 9);
        nib.set(0, 0, 0, 3);
        nib.set(15, 15, 15, 15);
        assert_eq!(nib.as_bytes()[0], 0x93);
        let light = LightData {
            sky: vec![
                LightUpdate {
                    section: 0,
                    data: Some(nib.clone()),
                },
                LightUpdate {
                    section: 25,
                    data: None,
                },
            ],
            block: vec![LightUpdate {
                section: 9,
                data: Some(nib),
            }],
        };
        let mut bytes = Vec::new();
        light.encode(&mut bytes);
        let mut input = bytes.as_slice();
        let back = LightData::decode(&mut input, &cfg).unwrap();
        assert!(input.is_empty());
        let mut sky = back.sky.clone();
        sky.sort_by_key(|u| u.section);
        assert_eq!(sky, light.sky);
        assert_eq!(back.block, light.block);
    }

    #[test]
    fn light_rejects_out_of_range_sections_and_bad_counts() {
        let cfg = cfg();
        let mut bytes = Vec::new();
        write_bitset(&mut bytes, &[26]);
        assert!(LightData::decode(&mut bytes.as_slice(), &cfg).is_err());
        let mut bytes = Vec::new();
        write_bitset(&mut bytes, &[1]);
        for _ in 0..3 {
            write_bitset(&mut bytes, &[]);
        }
        bytes.extend([0, 0]);
        assert!(LightData::decode(&mut bytes.as_slice(), &cfg).is_err());
        assert!(LightData::decode(&mut [0xffu8; 40].as_slice(), &cfg).is_err());
    }
}
