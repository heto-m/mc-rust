/// SplitMix64: a small, fast generator for gameplay sound rolls that need no vanilla parity.
#[derive(Debug, Clone)]
pub struct FastRandom {
    state: u64,
}

impl FastRandom {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `0..bound`; `bound == 0` yields 0.
    pub fn below(&mut self, bound: u32) -> u32 {
        (((self.next_u64() >> 32) * u64::from(bound)) >> 32) as u32
    }

    pub fn f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    pub fn f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draws_stay_in_range() {
        let mut r = FastRandom::new(9);
        for _ in 0..10_000 {
            assert!(r.below(7) < 7);
            let f = r.f32();
            assert!((0.0..1.0).contains(&f));
            assert!((0.0..1.0).contains(&r.f64()));
        }
        assert_eq!(r.below(0), 0);
    }

    #[test]
    fn below_is_roughly_uniform() {
        let mut r = FastRandom::new(1);
        let mut counts = [0u32; 4];
        for _ in 0..40_000 {
            counts[r.below(4) as usize] += 1;
        }
        assert!(counts.iter().all(|&c| (9_000..11_000).contains(&c)));
    }
}
