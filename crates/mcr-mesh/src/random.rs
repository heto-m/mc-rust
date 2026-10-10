//! Vanilla's position seed, Java-compatible generator and block offsets.

const MULTIPLIER: i64 = 0x5_DEEC_E66D;
const MASK: i64 = (1 << 48) - 1;

/// `Mth.getSeed(x, y, z)`.
pub fn mth_get_seed(x: i32, y: i32, z: i32) -> i64 {
    let seed = i64::from(x.wrapping_mul(3_129_871))
        ^ i64::from(z).wrapping_mul(116_129_781)
        ^ i64::from(y);
    seed.wrapping_mul(seed)
        .wrapping_mul(42_317_861)
        .wrapping_add(seed.wrapping_mul(11))
        >> 16
}

/// `SingleThreadedRandomSource` / `java.util.Random`.
#[derive(Debug, Clone, Copy, Default)]
pub struct JavaRandom {
    seed: i64,
}

impl JavaRandom {
    pub fn new(seed: i64) -> Self {
        let mut r = Self { seed: 0 };
        r.set_seed(seed);
        r
    }

    pub fn set_seed(&mut self, seed: i64) {
        self.seed = (seed ^ MULTIPLIER) & MASK;
    }

    pub fn next(&mut self, bits: u32) -> i32 {
        self.seed = self.seed.wrapping_mul(MULTIPLIER).wrapping_add(11) & MASK;
        (self.seed >> (48 - bits)) as i32
    }

    pub fn next_int(&mut self, bound: i32) -> i32 {
        debug_assert!(bound > 0);
        if bound & (bound - 1) == 0 {
            return ((i64::from(bound) * i64::from(self.next(31))) >> 31) as i32;
        }
        loop {
            let sample = self.next(31);
            let modulo = sample % bound;
            if sample.wrapping_sub(modulo).wrapping_add(bound - 1) >= 0 {
                return modulo;
            }
        }
    }

    pub fn next_long(&mut self) -> i64 {
        let upper = i64::from(self.next(32));
        let lower = i64::from(self.next(32));
        (upper << 32).wrapping_add(lower)
    }

    /// `(next(26) << 27 + next(27)) * 1.110223E-16F`, with the constant rounded to f32 like vanilla.
    pub fn next_double(&mut self) -> f64 {
        let upper = i64::from(self.next(26));
        let lower = i64::from(self.next(27));
        ((upper << 27) + lower) as f64 * f64::from(1.110_223e-16_f32)
    }
}

pub const OFFSET_NONE: u8 = 0;
pub const OFFSET_XZ: u8 = 1;
pub const OFFSET_XYZ: u8 = 2;

/// Horizontal and vertical maxima of a block's offset.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OffsetSpec {
    pub kind: u8,
    pub horizontal: f32,
    pub vertical: f32,
}

impl OffsetSpec {
    pub const NONE: Self = Self {
        kind: OFFSET_NONE,
        horizontal: 0.0,
        vertical: 0.0,
    };
}

/// Offset type of a block by name (vanilla `offsetType`), with its per-block maxima.
pub fn offset_spec(block: &str) -> OffsetSpec {
    let name = block.strip_prefix("minecraft:").unwrap_or(block);
    let kind = match name {
        "short_grass" | "fern" | "short_dry_grass" | "tall_dry_grass" | "small_dripleaf" => {
            OFFSET_XYZ
        }
        "mangrove_propagule" | "tall_seagrass" | "dandelion" | "golden_dandelion"
        | "torchflower" | "poppy" | "blue_orchid" | "allium" | "azure_bluet" | "red_tulip"
        | "orange_tulip" | "white_tulip" | "pink_tulip" | "oxeye_daisy" | "cornflower"
        | "wither_rose" | "lily_of_the_valley" | "sunflower" | "lilac" | "rose_bush" | "peony"
        | "tall_grass" | "large_fern" | "pitcher_plant" | "bamboo_sapling" | "bamboo"
        | "warped_roots" | "nether_sprouts" | "crimson_roots" | "pointed_dripstone"
        | "sulfur_spike" | "hanging_roots" | "open_eyeblossom" | "closed_eyeblossom" => OFFSET_XZ,
        _ => return OffsetSpec::NONE,
    };
    OffsetSpec {
        kind,
        horizontal: if matches!(name, "pointed_dripstone" | "sulfur_spike") {
            0.125
        } else {
            0.25
        },
        vertical: if name == "small_dripleaf" { 0.1 } else { 0.2 },
    }
}

/// Vanilla `getOffset` for the block at world column `(x, z)`.
pub fn block_offset(spec: OffsetSpec, x: i32, z: i32) -> [f32; 3] {
    if spec.kind == OFFSET_NONE {
        return [0.0; 3];
    }
    let seed = mth_get_seed(x, 0, z);
    let max = f64::from(spec.horizontal);
    let axis = |bits: i64| ((((bits & 15) as f32) / 15.0_f32) as f64 - 0.5) * 0.5;
    let ox = axis(seed).clamp(-max, max);
    let oz = axis(seed >> 8).clamp(-max, max);
    let oy = if spec.kind == OFFSET_XYZ {
        (f64::from(((seed >> 4 & 15) as f32) / 15.0_f32) - 1.0) * f64::from(spec.vertical)
    } else {
        0.0
    };
    [ox as f32, oy as f32, oz as f32]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeds_match_vanilla() {
        for (p, seed) in [
            ((0, 0, 0), 0),
            ((1, 0, 0), 133_076_631_897_947),
            ((5, 64, -3), 83_424_779_586_503),
            ((-100, 70, 250), -135_864_797_935_757),
            ((12345, -60, -98765), 21_765_384_610_372),
            ((3, 3, 3), -20_142_322_793_544),
        ] {
            assert_eq!(mth_get_seed(p.0, p.1, p.2), seed, "{p:?}");
        }
    }

    #[test]
    fn variant_draws_match_java_random() {
        let draw = |p: (i32, i32, i32), bound| {
            JavaRandom::new(mth_get_seed(p.0, p.1, p.2)).next_int(bound)
        };
        assert_eq!(draw((0, 0, 0), 4), 2);
        assert_eq!(draw((1, 0, 0), 4), 0);
        assert_eq!(draw((-100, 70, 250), 4), 1);
        assert_eq!(draw((1, 0, 0), 3), 1);
        assert_eq!(draw((5, 64, -3), 3), 2);
        assert_eq!(draw((-100, 70, 250), 3), 2);
        assert_eq!(
            JavaRandom::new(mth_get_seed(0, 0, 0)).next_long(),
            -4_962_768_465_676_381_896
        );
        assert_eq!(
            JavaRandom::new(mth_get_seed(12345, -60, -98765)).next_long(),
            2_707_098_702_388_896_123
        );
    }

    #[test]
    fn offsets_match_vanilla() {
        let xyz = offset_spec("minecraft:short_grass");
        let near = |a: [f32; 3], b: [f64; 3]| {
            a.iter()
                .zip(b)
                .all(|(x, y)| (f64::from(*x) - y).abs() < 1e-6)
        };
        assert!(near(block_offset(xyz, 0, 0), [-0.25, -0.2, -0.25]));
        assert!(near(
            block_offset(xyz, 1, 0),
            [0.116_666_674_6, -0.133_333_333_3, -0.016_666_665_67]
        ));
        assert!(near(
            block_offset(offset_spec("minecraft:poppy"), 5, -3),
            [0.216_666_668_7, 0.0, -0.183_333_329_9]
        ));
        assert_eq!(offset_spec("minecraft:stone"), OffsetSpec::NONE);
        assert_eq!(offset_spec("minecraft:pointed_dripstone").horizontal, 0.125);
    }
}
