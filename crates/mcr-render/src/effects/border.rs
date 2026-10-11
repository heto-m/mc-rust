use super::sky::FxVertex;

/// The vertical walls of the world border, as vanilla's `WorldBorderRenderer` draws them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldBorderVisual {
    pub min_x: f64,
    pub max_x: f64,
    pub min_z: f64,
    pub max_z: f64,
    /// Linear RGB of the border status: growing, shrinking or stationary.
    pub color: [f32; 3],
    /// `(1 - distance / render distance)^4`, already clamped.
    pub alpha: f32,
    /// Texture scroll in `[0, 1)`: vanilla's `millis % 3000 / 3000`.
    pub scroll: f32,
}

pub const MAX_BORDER_QUADS: usize = 4;
const INSET: f64 = 0.01;

impl WorldBorderVisual {
    /// Vanilla shows the border only when the camera is within `distance` of one of its walls.
    pub fn visible_from(&self, camera: [f64; 3], distance: f64) -> bool {
        let [x, _, z] = camera;
        let near_inside = !(x < self.max_x - distance
            && x > self.min_x + distance
            && z < self.max_z - distance
            && z > self.min_z + distance);
        let near_outside = !(x < self.min_x - distance
            || x > self.max_x + distance
            || z < self.min_z - distance
            || z > self.max_z + distance);
        near_inside && near_outside
    }
}

/// Up to four camera-relative wall quads within `distance` of the camera.
pub(crate) fn build(b: &WorldBorderVisual, camera: [f64; 3], distance: f64) -> Vec<FxVertex> {
    let mut out = Vec::with_capacity(MAX_BORDER_QUADS * 4);
    let values = [b.min_x, b.max_x, b.min_z, b.max_z, distance];
    if !values.iter().all(|v| v.is_finite()) || b.alpha <= 0.0 || distance <= 0.0 {
        return out;
    }
    let [cx, cy, cz] = camera;
    let (min_x, max_x) = (b.min_x + INSET, b.max_x - INSET);
    let (min_z, max_z) = (b.min_z + INSET, b.max_z - INSET);
    let span = |c: f64, lo: f64, hi: f64| {
        (
            (c - distance).floor().max(lo),
            (c + distance).ceil().min(hi),
        )
    };
    let (x0, x1) = span(cx, min_x, max_x);
    let (z0, z1) = span(cz, min_z, max_z);
    let half_height = distance as f32;
    let v_top = (-(cy * 0.5).rem_euclid(1.0)) as f32 + b.scroll;
    let v_bottom = v_top + half_height;
    let color = [b.color[0], b.color[1], b.color[2], b.alpha.clamp(0.0, 1.0)];
    let mut wall = |a: [f64; 2], c: [f64; 2], along: f64| {
        let u0 = ((along.floor() as i64 & 1) as f32) * 0.5 + b.scroll;
        let len = ((c[0] - a[0]).abs() + (c[1] - a[1]).abs()) as f32;
        let u1 = u0 + len / 2.0;
        let rel = |p: [f64; 2], y: f32| [(p[0] - cx) as f32, y, (p[1] - cz) as f32];
        out.push(FxVertex::new(rel(a, -half_height), [u0, v_bottom], color));
        out.push(FxVertex::new(rel(c, -half_height), [u1, v_bottom], color));
        out.push(FxVertex::new(rel(c, half_height), [u1, v_top], color));
        out.push(FxVertex::new(rel(a, half_height), [u0, v_top], color));
    };
    if x1 > x0 {
        if max_z - cz < distance {
            wall([x0, max_z], [x1, max_z], x0);
        }
        if cz - min_z < distance {
            wall([x1, min_z], [x0, min_z], x0);
        }
    }
    if z1 > z0 {
        if cx - min_x < distance {
            wall([min_x, z0], [min_x, z1], z0);
        }
        if max_x - cx < distance {
            wall([max_x, z1], [max_x, z0], z0);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn border(size: f64) -> WorldBorderVisual {
        WorldBorderVisual {
            min_x: -size / 2.0,
            max_x: size / 2.0,
            min_z: -size / 2.0,
            max_z: size / 2.0,
            color: [0.1, 0.6, 1.0],
            alpha: 1.0,
            scroll: 0.0,
        }
    }

    #[test]
    fn only_walls_within_render_distance_are_built() {
        let b = border(100.0);
        assert_eq!(build(&b, [45.0, 64.0, 0.0], 16.0).len(), 4);
        assert_eq!(build(&b, [45.0, 64.0, 45.0], 16.0).len(), 8);
        assert!(build(&b, [0.0, 64.0, 0.0], 16.0).is_empty());
    }

    #[test]
    fn visibility_matches_vanilla_band_around_the_walls() {
        let b = border(100.0);
        assert!(b.visible_from([45.0, 0.0, 0.0], 16.0));
        assert!(b.visible_from([60.0, 0.0, 0.0], 16.0));
        assert!(!b.visible_from([0.0, 0.0, 0.0], 16.0));
        assert!(!b.visible_from([90.0, 0.0, 0.0], 16.0));
    }

    #[test]
    fn hostile_values_build_nothing() {
        let mut b = border(100.0);
        b.max_x = f64::NAN;
        assert!(build(&b, [45.0, 64.0, 0.0], 16.0).is_empty());
    }
}
