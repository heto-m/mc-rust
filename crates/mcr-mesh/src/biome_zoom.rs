//! Vanilla's `BiomeManager` zoom: block-resolution biomes from the quart-resolution grid.

fn lcg(rval: i64, c: i64) -> i64 {
    rval.wrapping_mul(
        rval.wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407),
    )
    .wrapping_add(c)
}

fn fiddle(rval: i64) -> f64 {
    ((rval >> 24).rem_euclid(1024) as f64 / 1024.0 - 0.5) * 0.9
}

fn fiddled_distance(seed: i64, qx: i32, qy: i32, qz: i32, dx: f64, dy: f64, dz: f64) -> f64 {
    let (x, y, z) = (i64::from(qx), i64::from(qy), i64::from(qz));
    let mut r = seed;
    for c in [x, y, z, x, y, z] {
        r = lcg(r, c);
    }
    let fx = fiddle(r);
    r = lcg(r, seed);
    let fy = fiddle(r);
    r = lcg(r, seed);
    let fz = fiddle(r);
    let sq = |v: f64| v * v;
    sq(dz + fz) + sq(dy + fy) + sq(dx + fx)
}

/// The biome of the block at world `(x, y, z)`; `lookup` returns the biome at a world quart position.
pub fn zoom_biome(
    seed: i64,
    x: i32,
    y: i32,
    z: i32,
    mut lookup: impl FnMut(i32, i32, i32) -> u16,
) -> u16 {
    let (ax, ay, az) = (x - 2, y - 2, z - 2);
    let (px, py, pz) = (ax >> 2, ay >> 2, az >> 2);
    let corners: [u16; 8] = std::array::from_fn(|i| {
        lookup(
            px + i32::from(i & 4 != 0),
            py + i32::from(i & 2 != 0),
            pz + i32::from(i & 1 != 0),
        )
    });
    if corners.iter().all(|&c| c == corners[0]) {
        return corners[0];
    }
    let (fx, fy, fz) = (
        f64::from(ax & 3) / 4.0,
        f64::from(ay & 3) / 4.0,
        f64::from(az & 3) / 4.0,
    );
    let mut best = 0;
    let mut best_d = f64::INFINITY;
    for i in 0..8usize {
        let (xe, ye, ze) = (i & 4 == 0, i & 2 == 0, i & 1 == 0);
        let d = fiddled_distance(
            seed,
            if xe { px } else { px + 1 },
            if ye { py } else { py + 1 },
            if ze { pz } else { pz + 1 },
            if xe { fx } else { fx - 1.0 },
            if ye { fy } else { fy - 1.0 },
            if ze { fz } else { fz - 1.0 },
        );
        if best_d > d {
            best = i;
            best_d = d;
        }
    }
    corners[best]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_vanilla_reference() {
        let grid = |x: i32, y: i32, z: i32| (x * 31 + y * 17 + z * 13).rem_euclid(5) as u16;
        let expect = [
            (-100, -20, 7, 2),
            (-63, -15, 60, 3),
            (-26, -10, 113, 3),
            (11, -5, 166, 2),
            (48, 0, 219, 4),
            (85, 5, 272, 2),
            (122, 10, 325, 2),
            (159, 15, 378, 0),
            (196, 20, 431, 4),
            (233, 25, 484, 0),
            (270, 30, 537, 3),
            (307, 35, 590, 3),
        ];
        for (x, y, z, id) in expect {
            assert_eq!(zoom_biome(-4_172_144_997_902_289_642, x, y, z, grid), id);
        }
    }

    #[test]
    fn uniform_corners_short_circuit() {
        assert_eq!(zoom_biome(7, 5, 5, 5, |_, _, _| 3), 3);
    }

    #[test]
    fn mixed_corners_are_deterministic_and_pick_a_corner() {
        let grid = |x: i32, _y: i32, _z: i32| if x < 2 { 1 } else { 2 };
        for x in 0..16 {
            let a = zoom_biome(12345, x, 4, 4, grid);
            assert_eq!(a, zoom_biome(12345, x, 4, 4, grid));
            assert!(a == 1 || a == 2);
        }
        assert_eq!(zoom_biome(12345, 0, 4, 4, grid), 1);
        assert_eq!(zoom_biome(12345, 15, 4, 4, grid), 2);
    }
}
