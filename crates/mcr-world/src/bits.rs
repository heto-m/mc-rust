//! Packed entry storage where an entry never spans two longs (`SimpleBitStorage` layout).

pub(crate) const MAX_BITS: u8 = 32;

pub(crate) const fn values_per_long(bits: u8) -> usize {
    64 / bits as usize
}

pub(crate) const fn long_count(bits: u8, entries: usize) -> usize {
    entries.div_ceil(values_per_long(bits))
}

pub(crate) const fn mask(bits: u8) -> u64 {
    (1u64 << bits) - 1
}

pub(crate) const fn ceil_log2(n: u32) -> u8 {
    if n <= 1 {
        0
    } else {
        (32 - (n - 1).leading_zeros()) as u8
    }
}

/// `floor(index / values_per_long)` as `(index * RECIP[bits]) >> 32`; exact for index < 2^16.
const RECIP: [u64; MAX_BITS as usize + 1] = {
    let mut table = [0u64; MAX_BITS as usize + 1];
    let mut bits = 1;
    while bits <= MAX_BITS as usize {
        table[bits] = (1u64 << 32) / values_per_long(bits as u8) as u64 + 1;
        bits += 1;
    }
    table
};

#[inline(always)]
fn locate(bits: u8, index: usize) -> (usize, u32) {
    let vpl = values_per_long(bits);
    let cell = ((index as u64 * RECIP[bits as usize]) >> 32) as usize;
    (cell, ((index - cell * vpl) * bits as usize) as u32)
}

#[inline]
pub(crate) fn get(data: &[u64], bits: u8, index: usize) -> u32 {
    let (cell, shift) = locate(bits, index);
    ((data[cell] >> shift) & mask(bits)) as u32
}

#[inline]
pub(crate) fn set(data: &mut [u64], bits: u8, index: usize, value: u32) {
    let (cell, shift) = locate(bits, index);
    let m = mask(bits);
    data[cell] = (data[cell] & !(m << shift)) | ((value as u64 & m) << shift);
}

pub(crate) trait Slot: Copy {
    fn from_word(v: u64) -> Self;
    fn word(self) -> u64;
}

impl Slot for u16 {
    #[inline(always)]
    fn from_word(v: u64) -> Self {
        v as u16
    }
    #[inline(always)]
    fn word(self) -> u64 {
        self as u64
    }
}

impl Slot for u32 {
    #[inline(always)]
    fn from_word(v: u64) -> Self {
        v as u32
    }
    #[inline(always)]
    fn word(self) -> u64 {
        self as u64
    }
}

pub(crate) fn pack<T: Slot>(values: &[T], bits: u8) -> Box<[u64]> {
    let vpl = values_per_long(bits);
    let m = mask(bits);
    values
        .chunks(vpl)
        .map(|chunk| {
            chunk
                .iter()
                .rev()
                .fold(0u64, |acc, &v| (acc << bits) | (v.word() & m))
        })
        .collect()
}

pub(crate) fn unpack<T: Slot>(data: &[u64], bits: u8, out: &mut [T]) {
    match bits {
        1 => unpack_const::<T, 1>(data, out),
        2 => unpack_const::<T, 2>(data, out),
        3 => unpack_const::<T, 3>(data, out),
        4 => unpack_const::<T, 4>(data, out),
        5 => unpack_const::<T, 5>(data, out),
        6 => unpack_const::<T, 6>(data, out),
        7 => unpack_const::<T, 7>(data, out),
        8 => unpack_const::<T, 8>(data, out),
        _ => unpack_dyn(data, bits, out),
    }
}

#[inline(always)]
fn unpack_const<T: Slot, const BITS: u8>(data: &[u64], out: &mut [T]) {
    let vpl = values_per_long(BITS);
    let m = mask(BITS);
    for (&word, chunk) in data.iter().zip(out.chunks_mut(vpl)) {
        let mut w = word;
        for slot in chunk {
            *slot = T::from_word(w & m);
            w >>= BITS;
        }
    }
}

fn unpack_dyn<T: Slot>(data: &[u64], bits: u8, out: &mut [T]) {
    let vpl = values_per_long(bits);
    let m = mask(bits);
    for (&word, chunk) in data.iter().zip(out.chunks_mut(vpl)) {
        let mut w = word;
        for slot in chunk {
            *slot = T::from_word(w & m);
            w >>= bits;
        }
    }
}

pub(crate) fn max_entry(data: &[u64], bits: u8, entries: usize) -> u32 {
    let vpl = values_per_long(bits);
    let m = mask(bits);
    let mut max = 0u64;
    for (&word, remaining) in data
        .iter()
        .zip((0..entries).step_by(vpl).map(|i| entries - i))
    {
        let mut w = word;
        for _ in 0..vpl.min(remaining) {
            max = max.max(w & m);
            w >>= bits;
        }
    }
    max as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pattern(bits: u8, n: usize) -> Vec<u16> {
        let m = mask(bits.min(16));
        (0..n)
            .map(|i| ((i as u64).wrapping_mul(2654435761) & m) as u16)
            .collect()
    }

    #[test]
    fn reciprocal_matches_division_for_every_width() {
        for bits in 1..=MAX_BITS {
            let vpl = values_per_long(bits);
            for index in 0..4096usize {
                let (cell, shift) = locate(bits, index);
                assert_eq!(cell, index / vpl, "bits {bits} index {index}");
                assert_eq!(shift as usize, (index % vpl) * bits as usize);
            }
        }
    }

    #[test]
    fn pack_get_set_unpack_round_trip_for_bits_1_to_15() {
        for bits in 1..=15u8 {
            for n in [64usize, 4096] {
                let values = pattern(bits, n);
                let mut data = pack(&values, bits);
                assert_eq!(data.len(), long_count(bits, n));
                for (i, &v) in values.iter().enumerate() {
                    assert_eq!(get(&data, bits, i), v as u32, "bits {bits} i {i}");
                }
                let mut out = vec![0u16; n];
                unpack(&data, bits, &mut out);
                assert_eq!(out, values);

                let top = mask(bits) as u32;
                for i in (0..n).step_by(7) {
                    set(&mut data, bits, i, top);
                }
                for (i, &v) in values.iter().enumerate() {
                    let want = if i % 7 == 0 { top } else { v as u32 };
                    assert_eq!(get(&data, bits, i), want);
                }
            }
        }
    }

    #[test]
    fn entries_never_span_longs() {
        let bits = 5;
        assert_eq!(values_per_long(bits), 12);
        let mut data = vec![0u64; long_count(bits, 4096)];
        set(&mut data, bits, 12, 31);
        assert_eq!(data[0], 0);
        assert_eq!(data[1], 31);
        assert_eq!(data[1] >> 60, 0);
    }

    #[test]
    fn max_entry_ignores_padding_bits() {
        let mut data = vec![0u64; long_count(5, 64)];
        set(&mut data, 5, 63, 9);
        *data.last_mut().unwrap() |= 31 << 55;
        assert_eq!(max_entry(&data, 5, 64), 9);
    }

    #[test]
    fn ceil_log2_matches_vanilla() {
        assert_eq!(ceil_log2(1), 0);
        assert_eq!(ceil_log2(2), 1);
        assert_eq!(ceil_log2(35723), 16);
        assert_eq!(ceil_log2(65536), 16);
        assert_eq!(ceil_log2(65537), 17);
    }
}
