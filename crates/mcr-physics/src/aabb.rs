#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

impl Aabb {
    pub fn new(min: [f64; 3], max: [f64; 3]) -> Self {
        Self { min, max }
    }

    pub fn move_by(&self, d: [f64; 3]) -> Self {
        Self {
            min: [self.min[0] + d[0], self.min[1] + d[1], self.min[2] + d[2]],
            max: [self.max[0] + d[0], self.max[1] + d[1], self.max[2] + d[2]],
        }
    }

    pub fn expand_towards(&self, d: [f64; 3]) -> Self {
        let mut r = *self;
        for a in 0..3 {
            if d[a] < 0.0 {
                r.min[a] += d[a];
            } else if d[a] > 0.0 {
                r.max[a] += d[a];
            }
        }
        r
    }

    pub fn inflate(&self, v: f64) -> Self {
        Self {
            min: [self.min[0] - v, self.min[1] - v, self.min[2] - v],
            max: [self.max[0] + v, self.max[1] + v, self.max[2] + v],
        }
    }

    pub fn intersects(&self, o: &Aabb) -> bool {
        (0..3).all(|a| self.min[a] < o.max[a] && self.max[a] > o.min[a])
    }

    pub fn center(&self) -> [f64; 3] {
        [
            self.min[0] + (self.max[0] - self.min[0]) * 0.5,
            self.min[1] + (self.max[1] - self.min[1]) * 0.5,
            self.min[2] + (self.max[2] - self.min[2]) * 0.5,
        ]
    }

    pub fn contains(&self, p: [f64; 3]) -> bool {
        (0..3).all(|a| p[a] >= self.min[a] && p[a] < self.max[a])
    }

    pub fn segment_hits(&self, from: [f64; 3], to: [f64; 3]) -> bool {
        let (mut t0, mut t1) = (0.0f64, 1.0f64);
        for a in 0..3 {
            let d = to[a] - from[a];
            if d.abs() < 1e-12 {
                if from[a] < self.min[a] || from[a] > self.max[a] {
                    return false;
                }
            } else {
                let mut ta = (self.min[a] - from[a]) / d;
                let mut tb = (self.max[a] - from[a]) / d;
                if ta > tb {
                    std::mem::swap(&mut ta, &mut tb);
                }
                t0 = t0.max(ta);
                t1 = t1.min(tb);
                if t0 > t1 {
                    return false;
                }
            }
        }
        true
    }
}
