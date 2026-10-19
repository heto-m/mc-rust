use crate::cube::Vertex;
use crate::model::Model;

/// `java.util.Random`, which vanilla's thread-local random source reproduces.
pub(crate) struct JavaRandom(u64);

const MASK: u64 = (1 << 48) - 1;

impl JavaRandom {
    pub(crate) fn new(seed: u64) -> Self {
        JavaRandom((seed ^ 0x5DEECE66D) & MASK)
    }

    fn next(&mut self, bits: u32) -> i32 {
        self.0 = self.0.wrapping_mul(0x5DEECE66D).wrapping_add(0xB) & MASK;
        (self.0 >> (48 - bits)) as i32
    }

    pub(crate) fn next_float(&mut self) -> f32 {
        self.next(24) as f32 / (1 << 24) as f32
    }

    pub(crate) fn next_int(&mut self, bound: i32) -> i32 {
        if bound & (bound - 1) == 0 {
            return ((i64::from(bound) * i64::from(self.next(31))) >> 31) as i32;
        }
        loop {
            let bits = self.next(31);
            let val = bits % bound;
            if bits.wrapping_sub(val).wrapping_add(bound - 1) >= 0 {
                return val;
            }
        }
    }
}

/// The lightning bolt of `LightningBoltRenderer`: eight 16-block segments in four widening layers.
pub fn lightning_model(seed: u32) -> Model {
    let seed = u64::from(seed);
    let mut rng = JavaRandom::new(seed);
    let (mut xs, mut zs) = ([0f32; 8], [0f32; 8]);
    let (mut xo, mut zo) = (0f32, 0f32);
    for i in (0..8).rev() {
        xs[i] = xo;
        zs[i] = zo;
        xo += (rng.next_int(11) - 5) as f32;
        zo += (rng.next_int(11) - 5) as f32;
    }
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for layer in 0..4 {
        let mut rnd = JavaRandom::new(seed);
        for branch in 0..3 {
            let trunk = branch == 0;
            let start = 7 - branch;
            let end = if trunk { 0 } else { start - 3 + 1 };
            let mut sx = xs[start] - xo;
            let mut sz = zs[start] - zo;
            for seg in (end..=start).rev() {
                let (ex, ez) = (sx, sz);
                if trunk {
                    sx += (rnd.next_int(11) - 5) as f32;
                    sz += (rnd.next_int(11) - 5) as f32;
                } else {
                    sx += (rnd.next_int(31) - 15) as f32;
                    sz += (rnd.next_int(31) - 15) as f32;
                }
                let mut top = 0.1 + layer as f32 * 0.2;
                let mut bottom = top;
                if trunk {
                    top *= seg as f32 * 0.1 + 1.0;
                    bottom *= (seg as f32 - 1.0) * 0.1 + 1.0;
                }
                for (rx, rz, lx, lz) in [
                    (false, false, true, false),
                    (true, false, true, true),
                    (true, true, false, true),
                    (false, true, false, false),
                ] {
                    let sign = |p: bool, r: f32| if p { r } else { -r };
                    let y0 = seg as f32 * 16.0;
                    let y1 = (seg + 1) as f32 * 16.0;
                    let corners = [
                        [sx + sign(rx, bottom), y0, sz + sign(rz, bottom)],
                        [ex + sign(rx, top), y1, ez + sign(rz, top)],
                        [ex + sign(lx, top), y1, ez + sign(lz, top)],
                        [sx + sign(lx, bottom), y0, sz + sign(lz, bottom)],
                    ];
                    let base = vertices.len() as u32;
                    for c in corners {
                        vertices.push(Vertex {
                            pos: c,
                            bone: 0,
                            normal: [0.0, 1.0, 0.0],
                            uv: [0.5, 0.5],
                        });
                    }
                    indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
                }
            }
        }
    }
    Model::from_geometry(vertices, indices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_random_matches_known_sequence() {
        let mut r = JavaRandom::new(42);
        assert_eq!(r.next_int(10), 0);
        assert_eq!(r.next_int(10), 3);
    }

    #[test]
    fn bolt_has_four_layers_of_quads() {
        let m = lightning_model(7);
        assert_eq!(m.vertices.len() % 4, 0);
        assert!(m.vertices.len() > 4 * 4 * 8);
    }
}
