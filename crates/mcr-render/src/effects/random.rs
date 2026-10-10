/// The `java.util.Random` LCG, which vanilla seeds for stars and per-column weather variation.
#[derive(Debug, Clone)]
pub(crate) struct JavaRandom {
    seed: u64,
    next_gaussian: Option<f64>,
}

const MULTIPLIER: u64 = 0x5_DEEC_E66D;
const MASK: u64 = (1 << 48) - 1;

impl JavaRandom {
    pub(crate) fn new(seed: i64) -> Self {
        Self {
            seed: (seed as u64 ^ MULTIPLIER) & MASK,
            next_gaussian: None,
        }
    }

    fn next(&mut self, bits: u32) -> i32 {
        self.seed = (self.seed.wrapping_mul(MULTIPLIER).wrapping_add(0xB)) & MASK;
        (self.seed >> (48 - bits)) as i64 as i32
    }

    pub(crate) fn next_f32(&mut self) -> f32 {
        self.next(24) as f32 / (1 << 24) as f32
    }

    pub(crate) fn next_f64(&mut self) -> f64 {
        let hi = i64::from(self.next(26));
        let lo = i64::from(self.next(27));
        ((hi << 27) + lo) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    pub(crate) fn next_gaussian(&mut self) -> f64 {
        if let Some(g) = self.next_gaussian.take() {
            return g;
        }
        loop {
            let v1 = 2.0 * self.next_f64() - 1.0;
            let v2 = 2.0 * self.next_f64() - 1.0;
            let s = v1 * v1 + v2 * v2;
            if s < 1.0 && s != 0.0 {
                let m = (-2.0 * s.ln() / s).sqrt();
                self.next_gaussian = Some(v2 * m);
                return v1 * m;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_java_random() {
        let mut r = JavaRandom::new(42);
        assert_eq!(r.next(32), -1_170_105_035);
        assert_eq!(r.next(32), 234_785_527);
    }

    #[test]
    fn floats_are_unit_range() {
        let mut r = JavaRandom::new(10842);
        for _ in 0..1000 {
            let f = r.next_f32();
            assert!((0.0..1.0).contains(&f));
        }
    }
}
