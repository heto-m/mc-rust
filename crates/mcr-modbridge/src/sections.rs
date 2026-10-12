//! mcr's reached-section set in shared memory, so Java culls block entities and entities that mcr's
//! visibility graph rules out (Java's own graph sees through its meshless sections).

use std::sync::atomic::{AtomicU64, Ordering};

use mcr_render::ReachedSections;

use crate::abi::{SEC_BITS, SEC_DIMS, SEC_MAX_BITS, SEC_ORIGIN, SEC_SEQ};

const HEADER_WORDS: usize = SEC_BITS / 8;
const BIT_WORDS: usize = SEC_MAX_BITS / 64;

const _: () = {
    assert!(SEC_SEQ == 0 && SEC_ORIGIN == 8 && SEC_DIMS == 20);
    assert!(SEC_BITS.is_multiple_of(8) && SEC_MAX_BITS.is_multiple_of(64));
};

/// Written by the render thread between Java's frames, read by Java during its frame.
#[derive(Debug)]
pub(crate) struct SectionShared {
    words: Box<[AtomicU64]>,
}

fn pair(a: i32, b: i32) -> u64 {
    let mut bytes = [0u8; 8];
    bytes[..4].copy_from_slice(&a.to_ne_bytes());
    bytes[4..].copy_from_slice(&b.to_ne_bytes());
    u64::from_ne_bytes(bytes)
}

impl SectionShared {
    pub fn new() -> Self {
        Self {
            words: (0..HEADER_WORDS + BIT_WORDS)
                .map(|_| AtomicU64::new(0))
                .collect(),
        }
    }

    pub fn addr(&self) -> i64 {
        self.words.as_ptr() as i64
    }

    /// Copies `set` for Java; `false` when it does not fit, so Java keeps its own culling.
    pub fn publish(&self, set: &ReachedSections<'_>) -> bool {
        let volume = set.dims.iter().map(|&d| u64::from(d)).product::<u64>();
        let words = volume.div_ceil(64) as usize;
        let dims = set.dims.map(|d| i32::try_from(d).unwrap_or(i32::MAX));
        if volume == 0 || volume > SEC_MAX_BITS as u64 || set.bits.len() < words {
            return false;
        }
        let [ox, oy, oz] = set.origin;
        let w = &self.words;
        w[1].store(pair(ox, oy), Ordering::Relaxed);
        w[2].store(pair(oz, dims[0]), Ordering::Relaxed);
        w[3].store(pair(dims[1], dims[2]), Ordering::Relaxed);
        for (dst, src) in w[HEADER_WORDS..HEADER_WORDS + words].iter().zip(set.bits) {
            dst.store(*src, Ordering::Relaxed);
        }
        w[0].fetch_add(1, Ordering::Release);
        true
    }

    #[cfg(test)]
    fn read_i32(&self, at: usize) -> i32 {
        let word = self.words[at / 8].load(Ordering::Acquire).to_ne_bytes();
        let o = at % 8;
        i32::from_ne_bytes(word[o..o + 4].try_into().unwrap())
    }

    #[cfg(test)]
    fn bit(&self, i: usize) -> bool {
        self.words[HEADER_WORDS + i / 64].load(Ordering::Acquire) >> (i % 64) & 1 != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_matches_the_java_offsets() {
        let shared = SectionShared::new();
        let bits = [0b1010u64, 1 << 63];
        let set = ReachedSections {
            origin: [-3, -4, 7],
            dims: [5, 4, 6],
            bits: &bits,
            generation: 9,
        };
        assert!(shared.publish(&set));
        assert_eq!(shared.words[0].load(Ordering::Acquire), 1);
        let ints: Vec<i32> = (0..6)
            .map(|i| shared.read_i32(SEC_ORIGIN + 4 * i))
            .collect();
        assert_eq!(ints, [-3, -4, 7, 5, 4, 6]);
        assert!(!shared.bit(0) && shared.bit(1) && shared.bit(3) && shared.bit(127));
        assert_eq!(shared.addr() % 8, 0);
    }

    #[test]
    fn oversized_or_short_sets_are_not_shared() {
        let shared = SectionShared::new();
        let huge = ReachedSections {
            origin: [0; 3],
            dims: [u32::MAX, 2, 2],
            bits: &[],
            generation: 1,
        };
        assert!(!shared.publish(&huge));
        let short = ReachedSections {
            origin: [0; 3],
            dims: [64, 1, 2],
            bits: &[0],
            generation: 1,
        };
        assert!(!shared.publish(&short));
        assert_eq!(shared.words[0].load(Ordering::Acquire), 0);
    }
}
