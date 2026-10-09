use mcr_buf::{read_varint, take, write_varint};

use crate::bits;
use crate::{Result, WorldError};

pub const BLOCK_ENTRIES: usize = 4096;
pub const BIOME_ENTRIES: usize = 64;

pub type BlockContainer = PalettedContainer<BLOCK_ENTRIES>;
pub type BiomeContainer = PalettedContainer<BIOME_ENTRIES>;

/// Index order is y, then z, then x.
#[inline]
pub const fn block_index(x: usize, y: usize, z: usize) -> usize {
    ((y & 15) << 8) | ((z & 15) << 4) | (x & 15)
}

/// Biomes are stored per 4x4x4 cell, same y, z, x order.
#[inline]
pub const fn biome_index(x: usize, y: usize, z: usize) -> usize {
    ((y & 3) << 4) | ((z & 3) << 2) | (x & 3)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    id_count: u32,
    min_bits: u8,
    max_indirect: u8,
    direct_bits: u8,
}

impl Limits {
    /// `min_bits` is the narrowest indirect width, `max_indirect` the widest before going direct.
    pub fn new(id_count: u32, min_bits: u8, max_indirect: u8) -> Self {
        Self {
            id_count,
            min_bits,
            max_indirect,
            direct_bits: bits::ceil_log2(id_count).max(max_indirect + 1),
        }
    }

    pub fn id_count(&self) -> u32 {
        self.id_count
    }

    pub fn direct_bits(&self) -> u8 {
        self.direct_bits
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteKind {
    Single,
    Indirect,
    Direct,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Repr {
    Single(u16),
    Indirect {
        palette: Vec<u16>,
        bits: u8,
        data: Box<[u64]>,
    },
    Direct {
        bits: u8,
        data: Box<[u64]>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PalettedContainer<const N: usize> {
    repr: Repr,
    limits: Limits,
}

impl<const N: usize> PalettedContainer<N> {
    pub fn filled(value: u16, limits: Limits) -> Self {
        Self {
            repr: Repr::Single(value),
            limits,
        }
    }

    /// Picks the narrowest representation for the distinct values present.
    pub fn from_values(values: &[u16; N], limits: Limits) -> Self {
        let mut slot_of = vec![u32::MAX; limits.id_count as usize];
        let mut palette: Vec<u16> = Vec::new();
        let mut indices = [0u16; N];
        for (out, &v) in indices.iter_mut().zip(values) {
            let slot = &mut slot_of[v as usize];
            if *slot == u32::MAX {
                *slot = palette.len() as u32;
                palette.push(v);
            }
            *out = *slot as u16;
        }
        let repr = if palette.len() == 1 {
            Repr::Single(palette[0])
        } else if palette.len() <= 1 << limits.max_indirect {
            let bits = bits::ceil_log2(palette.len() as u32).max(limits.min_bits);
            Repr::Indirect {
                data: bits::pack(&indices, bits),
                palette,
                bits,
            }
        } else {
            Repr::Direct {
                bits: limits.direct_bits,
                data: bits::pack(values, limits.direct_bits),
            }
        };
        Self { repr, limits }
    }

    pub fn kind(&self) -> PaletteKind {
        match self.repr {
            Repr::Single(_) => PaletteKind::Single,
            Repr::Indirect { .. } => PaletteKind::Indirect,
            Repr::Direct { .. } => PaletteKind::Direct,
        }
    }

    pub fn bits(&self) -> u8 {
        match self.repr {
            Repr::Single(_) => 0,
            Repr::Indirect { bits, .. } | Repr::Direct { bits, .. } => bits,
        }
    }

    /// Ids present in a single or indirect container; empty for direct.
    pub fn palette(&self) -> &[u16] {
        match &self.repr {
            Repr::Single(v) => std::slice::from_ref(v),
            Repr::Indirect { palette, .. } => palette,
            Repr::Direct { .. } => &[],
        }
    }

    #[inline]
    pub fn get(&self, index: usize) -> u16 {
        debug_assert!(index < N);
        match &self.repr {
            Repr::Single(v) => *v,
            Repr::Indirect {
                palette,
                bits,
                data,
            } => palette[bits::get(data, *bits, index) as usize],
            Repr::Direct { bits, data } => bits::get(data, *bits, index) as u16,
        }
    }

    /// Returns the previous value; widens the palette as needed.
    pub fn set(&mut self, index: usize, value: u16) -> u16 {
        assert!(index < N, "container index {index} out of range");
        match &mut self.repr {
            Repr::Single(old) => {
                let old = *old;
                if old == value {
                    return old;
                }
                let bits = self.limits.min_bits;
                let mut data = vec![0u64; bits::long_count(bits, N)].into_boxed_slice();
                bits::set(&mut data, bits, index, 1);
                self.repr = Repr::Indirect {
                    palette: vec![old, value],
                    bits,
                    data,
                };
                old
            }
            Repr::Indirect {
                palette,
                bits,
                data,
            } => {
                let slot = match palette.iter().position(|&p| p == value) {
                    Some(slot) => slot,
                    None if palette.len() < 1 << *bits => {
                        palette.push(value);
                        palette.len() - 1
                    }
                    None => return self.widen_and_set(index, value),
                };
                let old = palette[bits::get(data, *bits, index) as usize];
                bits::set(data, *bits, index, slot as u32);
                old
            }
            Repr::Direct { bits, data } => {
                let old = bits::get(data, *bits, index) as u16;
                bits::set(data, *bits, index, value as u32);
                old
            }
        }
    }

    #[cold]
    fn widen_and_set(&mut self, index: usize, value: u16) -> u16 {
        let Repr::Indirect {
            palette,
            bits: old_bits,
            data,
        } = &mut self.repr
        else {
            unreachable!("only a full indirect palette widens");
        };
        let mut slots = [0u16; N];
        bits::unpack(data, *old_bits, &mut slots);
        let old = palette[slots[index] as usize];
        if *old_bits < self.limits.max_indirect {
            let bits = *old_bits + 1;
            palette.push(value);
            slots[index] = (palette.len() - 1) as u16;
            *data = bits::pack(&slots, bits);
            *old_bits = bits;
        } else {
            for slot in &mut slots {
                *slot = palette[*slot as usize];
            }
            slots[index] = value;
            let bits = self.limits.direct_bits;
            self.repr = Repr::Direct {
                bits,
                data: bits::pack(&slots, bits),
            };
        }
        old
    }

    pub fn unpack_into(&self, out: &mut [u16; N]) {
        match &self.repr {
            Repr::Single(v) => out.fill(*v),
            Repr::Indirect {
                palette,
                bits,
                data,
            } => {
                bits::unpack(data, *bits, out);
                for slot in out.iter_mut() {
                    *slot = palette[*slot as usize];
                }
            }
            Repr::Direct { bits, data } => bits::unpack(data, *bits, out),
        }
    }

    pub fn decode(input: &mut &[u8], limits: Limits) -> Result<Self> {
        let declared = take(input, 1)?[0];
        let repr = if declared == 0 {
            let id = read_id(input, limits.id_count)?;
            Repr::Single(id)
        } else if declared <= limits.max_indirect {
            let bits = declared.max(limits.min_bits);
            let len = read_varint(input)?;
            let len = usize::try_from(len)
                .ok()
                .filter(|&l| l >= 1 && l <= 1 << bits)
                .ok_or(WorldError::PaletteTooLarge {
                    len: len.max(0) as usize,
                    bits,
                })?;
            let mut palette = Vec::with_capacity(len);
            for _ in 0..len {
                palette.push(read_id(input, limits.id_count)?);
            }
            let data = read_longs(input, bits, N)?;
            if len < 1 << bits {
                let max = bits::max_entry(&data, bits, N);
                if max as usize >= len {
                    return Err(WorldError::PaletteIndex {
                        index: max,
                        len: len as u32,
                    });
                }
            }
            Repr::Indirect {
                palette,
                bits,
                data,
            }
        } else {
            let bits = limits.direct_bits;
            let data = read_longs(input, bits, N)?;
            if limits.id_count < 1 << bits {
                let max = bits::max_entry(&data, bits, N);
                if max >= limits.id_count {
                    return Err(WorldError::IdOutOfRange {
                        id: max,
                        count: limits.id_count,
                    });
                }
            }
            Repr::Direct { bits, data }
        };
        Ok(Self { repr, limits })
    }

    pub fn encode(&self, out: &mut Vec<u8>) {
        match &self.repr {
            Repr::Single(v) => {
                out.push(0);
                write_varint(out, *v as i32);
            }
            Repr::Indirect {
                palette,
                bits,
                data,
            } => {
                out.push(*bits);
                write_varint(out, palette.len() as i32);
                for &id in palette {
                    write_varint(out, id as i32);
                }
                write_longs(out, data);
            }
            Repr::Direct { bits, data } => {
                out.push(*bits);
                write_longs(out, data);
            }
        }
    }
}

fn read_id(input: &mut &[u8], count: u32) -> Result<u16> {
    let id = read_varint(input)?;
    match u32::try_from(id) {
        Ok(id) if id < count => Ok(id as u16),
        _ => Err(WorldError::IdOutOfRange {
            id: id as u32,
            count,
        }),
    }
}

fn read_longs(input: &mut &[u8], bits: u8, entries: usize) -> Result<Box<[u64]>> {
    let count = bits::long_count(bits, entries);
    let raw = take(input, count * 8)?;
    Ok(raw
        .as_chunks::<8>()
        .0
        .iter()
        .map(|c| u64::from_be_bytes(*c))
        .collect())
}

fn write_longs(out: &mut Vec<u8>, data: &[u64]) {
    for word in data {
        out.extend_from_slice(&word.to_be_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLOCKS: u32 = 35723;

    fn block_limits() -> Limits {
        Limits::new(BLOCKS, 4, 8)
    }

    fn biome_limits() -> Limits {
        Limits::new(80, 1, 3)
    }

    fn distinct_values<const N: usize>(distinct: usize, count: u32) -> [u16; N] {
        let mut v = [0u16; N];
        for (i, slot) in v.iter_mut().enumerate() {
            *slot = (i % distinct) as u16 * (count / distinct as u32).max(1) as u16;
        }
        v
    }

    fn round_trip<const N: usize>(values: &[u16; N], limits: Limits) -> PalettedContainer<N> {
        let c = PalettedContainer::<N>::from_values(values, limits);
        let mut bytes = Vec::new();
        c.encode(&mut bytes);
        let mut input = bytes.as_slice();
        let back = PalettedContainer::<N>::decode(&mut input, limits).unwrap();
        assert!(input.is_empty());
        assert_eq!(back, c);
        let mut out = [0u16; N];
        back.unpack_into(&mut out);
        assert_eq!(&out, values);
        for (i, &v) in values.iter().enumerate() {
            assert_eq!(back.get(i), v);
        }
        back
    }

    #[test]
    fn index_order_is_y_z_x() {
        assert_eq!(block_index(1, 0, 0), 1);
        assert_eq!(block_index(0, 0, 1), 16);
        assert_eq!(block_index(0, 1, 0), 256);
        assert_eq!(block_index(15, 15, 15), 4095);
        assert_eq!(biome_index(1, 0, 0), 1);
        assert_eq!(biome_index(0, 0, 1), 4);
        assert_eq!(biome_index(0, 1, 0), 16);
    }

    #[test]
    fn single_value_wire_bytes() {
        let c = BlockContainer::filled(300, block_limits());
        let mut bytes = Vec::new();
        c.encode(&mut bytes);
        assert_eq!(bytes, [0, 0xac, 0x02]);
        let mut input = bytes.as_slice();
        let back = BlockContainer::decode(&mut input, block_limits()).unwrap();
        assert_eq!(back.kind(), PaletteKind::Single);
        assert_eq!(back.get(4095), 300);
    }

    #[test]
    fn indirect_has_no_array_length_prefix() {
        let mut v = [0u16; BLOCK_ENTRIES];
        v[5] = 9;
        let c = BlockContainer::from_values(&v, block_limits());
        let mut bytes = Vec::new();
        c.encode(&mut bytes);
        assert_eq!(bytes.len(), 1 + 1 + 2 + 256 * 8);
        assert_eq!(&bytes[..4], [4, 2, 0, 9]);
    }

    #[test]
    fn blocks_round_trip_every_palette_kind() {
        for distinct in [1, 2, 3, 16, 17, 32, 33, 64, 128, 129, 255, 256, 257, 4096] {
            let v = distinct_values::<BLOCK_ENTRIES>(distinct, BLOCKS);
            let c = round_trip(&v, block_limits());
            let expect = match distinct {
                1 => PaletteKind::Single,
                2..=256 => PaletteKind::Indirect,
                _ => PaletteKind::Direct,
            };
            assert_eq!(c.kind(), expect, "distinct {distinct}");
        }
    }

    #[test]
    fn biomes_round_trip_every_palette_kind() {
        for distinct in [1, 2, 3, 4, 5, 8, 9, 40, 64] {
            let v = distinct_values::<BIOME_ENTRIES>(distinct, 80);
            let c = round_trip(&v, biome_limits());
            let expect = match distinct {
                1 => PaletteKind::Single,
                2..=8 => PaletteKind::Indirect,
                _ => PaletteKind::Direct,
            };
            assert_eq!(c.kind(), expect, "distinct {distinct}");
        }
    }

    #[test]
    fn round_trips_for_direct_widths_9_to_16() {
        for bits in 9..=16u8 {
            let count = (1u32 << bits).min(65536);
            let limits = Limits::new(count, 4, 8);
            assert_eq!(limits.direct_bits(), bits);
            let v = distinct_values::<BLOCK_ENTRIES>((count as usize).min(4000), count);
            let c = round_trip(&v, limits);
            assert_eq!(c.bits(), bits);
        }
    }

    #[test]
    fn narrow_block_widths_are_read_as_four_bits() {
        let mut bytes = vec![2u8, 2, 0, 1];
        bytes.extend(std::iter::repeat_n(0u8, 256 * 8));
        let mut input = bytes.as_slice();
        let c = BlockContainer::decode(&mut input, block_limits()).unwrap();
        assert!(input.is_empty());
        assert_eq!(c.bits(), 4);
        assert_eq!(c.get(0), 0);
    }

    #[test]
    fn set_grows_through_every_stage() {
        let mut c = BlockContainer::filled(0, block_limits());
        let mut model = [0u16; BLOCK_ENTRIES];
        let mut seen_bits = Vec::new();
        for (i, slot) in model.iter_mut().enumerate() {
            let value = (i as u16 + 1) * 3;
            assert_eq!(c.set(i, value), *slot);
            *slot = value;
            if seen_bits.last() != Some(&c.bits()) {
                seen_bits.push(c.bits());
            }
        }
        assert_eq!(seen_bits, [4, 5, 6, 7, 8, 16]);
        let mut out = [0u16; BLOCK_ENTRIES];
        c.unpack_into(&mut out);
        assert_eq!(out, model);
        assert_eq!(c.set(10, 7), model[10]);
        assert_eq!(c.get(10), 7);
    }

    #[test]
    fn biome_set_grows_to_direct() {
        let mut c = BiomeContainer::filled(1, biome_limits());
        for i in 0..BIOME_ENTRIES {
            c.set(i, i as u16);
        }
        assert_eq!(c.kind(), PaletteKind::Direct);
        for i in 0..BIOME_ENTRIES {
            assert_eq!(c.get(i), i as u16);
        }
    }

    #[test]
    fn set_same_value_keeps_single() {
        let mut c = BlockContainer::filled(5, block_limits());
        assert_eq!(c.set(3, 5), 5);
        assert_eq!(c.kind(), PaletteKind::Single);
    }

    fn decode_block(bytes: &[u8]) -> Result<BlockContainer> {
        BlockContainer::decode(&mut &*bytes, block_limits())
    }

    #[test]
    fn palette_index_out_of_range_is_an_error() {
        let mut bytes = vec![4u8, 2, 0, 1];
        let mut data = vec![0u8; 256 * 8];
        data[7] = 0x02;
        bytes.extend(data);
        assert!(matches!(
            decode_block(&bytes),
            Err(WorldError::PaletteIndex { index: 2, len: 2 })
        ));
    }

    #[test]
    fn hostile_inputs_never_panic() {
        assert!(decode_block(&[]).is_err());
        assert!(decode_block(&[0]).is_err());
        assert!(decode_block(&[0, 0xff, 0xff, 0xff, 0xff, 0x0f]).is_err());
        assert!(decode_block(&[0, 0xff, 0xff, 0x03]).is_err());
        assert!(decode_block(&[4, 0]).is_err());
        assert!(decode_block(&[4, 17, 0]).is_err());
        assert!(decode_block(&[4, 0xff, 0xff, 0xff, 0xff, 0x0f]).is_err());
        assert!(decode_block(&[4, 1, 0, 1, 2, 3]).is_err());
        assert!(decode_block(&[16, 1, 2, 3]).is_err());
        let mut direct = vec![16u8];
        direct.extend(std::iter::repeat_n(0xffu8, 1024 * 8));
        assert!(matches!(
            decode_block(&direct),
            Err(WorldError::IdOutOfRange { .. })
        ));
        assert!(BlockContainer::decode(&mut &[255u8][..], block_limits()).is_err());
    }

    #[test]
    fn exhaustive_truncation_never_panics() {
        let v = distinct_values::<BLOCK_ENTRIES>(100, BLOCKS);
        let mut bytes = Vec::new();
        BlockContainer::from_values(&v, block_limits()).encode(&mut bytes);
        for cut in (0..bytes.len()).step_by(13) {
            assert!(decode_block(&bytes[..cut]).is_err());
        }
    }
}
