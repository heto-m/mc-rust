/// SplitMix64: small, fixed forever, so a case file never changes under a dependency bump.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    /// FNV-1a of the name.
    pub fn seeded(name: &str) -> Self {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in name.bytes() {
            h = (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
        }
        Self(h)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform in `lo..=hi`.
    pub fn range(&mut self, lo: i64, hi: i64) -> i64 {
        debug_assert!(lo <= hi);
        let span = (hi - lo) as u64 + 1;
        lo + (self.next_u64() % span) as i64
    }

    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n.max(1) as u64) as usize
    }

    /// True with probability `percent` / 100.
    pub fn chance(&mut self, percent: u32) -> bool {
        self.next_u64() % 100 < u64::from(percent)
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }

    /// An index into `weights`, chosen in proportion to them.
    pub fn weighted(&mut self, weights: &[u32]) -> usize {
        let total: u64 = weights.iter().map(|&w| u64::from(w)).sum();
        let mut roll = self.next_u64() % total.max(1);
        for (i, &w) in weights.iter().enumerate() {
            if roll < u64::from(w) {
                return i;
            }
            roll -= u64::from(w);
        }
        weights.len() - 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequence_is_pinned() {
        let mut r = Rng::new(0);
        assert_eq!(r.next_u64(), 0xe220_a839_7b1d_cdaf);
        assert_eq!(
            Rng::seeded("font").next_u64(),
            Rng::seeded("font").next_u64()
        );
        assert_ne!(
            Rng::seeded("font").next_u64(),
            Rng::seeded("fonts").next_u64()
        );
    }

    #[test]
    fn range_and_weights_stay_in_bounds() {
        let mut r = Rng::new(7);
        for _ in 0..1000 {
            assert!((3..=5).contains(&r.range(3, 5)));
            assert!(r.weighted(&[0, 1, 0]) == 1);
        }
    }
}
