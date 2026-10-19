//! Vanilla's 2D simplex noise, used for swamp grass colour and biome temperature.

use crate::random::JavaRandom;

const GRADIENT: [[i32; 2]; 16] = [
    [1, 1],
    [-1, 1],
    [1, -1],
    [-1, -1],
    [1, 0],
    [-1, 0],
    [1, 0],
    [-1, 0],
    [0, 1],
    [0, -1],
    [0, 1],
    [0, -1],
    [1, 1],
    [0, -1],
    [-1, 1],
    [0, -1],
];

const F2: f64 = 0.366_025_403_784_438_6;
const G2: f64 = 0.211_324_865_405_187_1;

#[derive(Debug, Clone)]
pub struct Simplex {
    perms: [u8; 256],
}

impl Simplex {
    /// `new SimplexNoise(new WorldgenRandom(new LegacyRandomSource(seed)), true)`.
    pub fn new(seed: i64) -> Self {
        Self::from_random(&mut JavaRandom::new(seed))
    }

    /// `new SimplexNoise(random, true)`, advancing `random` like vanilla so noises can share one.
    pub fn from_random(random: &mut JavaRandom) -> Self {
        for _ in 0..3 {
            random.next_double();
        }
        let mut perms: [u8; 256] = std::array::from_fn(|i| i as u8);
        for i in 0..256 {
            let offset = random.next_int(256 - i as i32) as usize;
            perms.swap(i, i + offset);
        }
        Self { perms }
    }

    fn permute(&self, x: i32) -> i32 {
        i32::from(self.perms[(x & 255) as usize])
    }

    fn corner(index: i32, x: f64, y: f64) -> f64 {
        let t = 0.5 - x * x - y * y;
        if t < 0.0 {
            return 0.0;
        }
        let g = GRADIENT[(index % 12) as usize];
        let t = t * t;
        t * t * (f64::from(g[0]) * x + f64::from(g[1]) * y)
    }

    pub fn get(&self, xin: f64, yin: f64) -> f32 {
        let s = (xin + yin) * F2;
        let i = (xin + s).floor() as i32;
        let j = (yin + s).floor() as i32;
        let t = f64::from(i + j) * G2;
        let x0 = xin - (f64::from(i) - t);
        let y0 = yin - (f64::from(j) - t);
        let (i1, j1) = if x0 > y0 { (1, 0) } else { (0, 1) };
        let x1 = x0 - f64::from(i1) + G2;
        let y1 = y0 - f64::from(j1) + G2;
        let x2 = x0 - 1.0 + 2.0 * G2;
        let y2 = y0 - 1.0 + 2.0 * G2;
        let (ii, jj) = (i & 255, j & 255);
        let gi0 = self.permute(ii + self.permute(jj));
        let gi1 = self.permute(ii + i1 + self.permute(jj + j1));
        let gi2 = self.permute(ii + 1 + self.permute(jj + 1));
        let n = Self::corner(gi0, x0, y0) + Self::corner(gi1, x1, y1) + Self::corner(gi2, x2, y2);
        (70.0 * n) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_reference_values() {
        let noise = Simplex::new(2345);
        for (x, z, expect) in REFERENCE {
            let got = noise.get(x * 0.0225, z * 0.0225);
            assert!((got - expect).abs() < 1e-6, "({x}, {z}): {got} vs {expect}");
        }
    }

    const REFERENCE: [(f64, f64, f32); 5] = [
        (0.0, 0.0, 0.0),
        (100.0, 200.0, -0.7511575),
        (-350.0, 40.0, 0.10542024),
        (1234.0, -5678.0, 0.17188638),
        (77.0, 77.0, 0.0019652105),
    ];
}
