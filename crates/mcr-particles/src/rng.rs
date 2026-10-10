/// Small xorshift generator; particle randomness only needs speed and a decent spread.
#[derive(Debug, Clone)]
pub(crate) struct Rng(u64);

impl Rng {
    pub(crate) fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    pub(crate) fn next_u32(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 32) as u32
    }

    /// In `[0, 1)`.
    pub(crate) fn f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / (1 << 24) as f32
    }

    pub(crate) fn f64(&mut self) -> f64 {
        f64::from(self.next_u32()) / 4_294_967_296.0
    }

    pub(crate) fn below(&mut self, n: u32) -> u32 {
        if n == 0 { 0 } else { self.next_u32() % n }
    }

    pub(crate) fn gaussian(&mut self) -> f64 {
        let u = self.f64().max(1e-12);
        let v = self.f64();
        (-2.0 * u.ln()).sqrt() * (std::f64::consts::TAU * v).cos()
    }

    pub(crate) fn bool(&mut self) -> bool {
        self.next_u32() & 1 == 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_hold() {
        let mut r = Rng::new(7);
        for _ in 0..10_000 {
            assert!((0.0..1.0).contains(&r.f32()));
            assert!((0.0..1.0).contains(&r.f64()));
            assert!(r.below(5) < 5);
            assert!(r.gaussian().is_finite());
        }
    }
}
