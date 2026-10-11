//! Block `SoundType` model derived from the break event name the data tables carry.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SoundType<'a> {
    family: &'a str,
    pub volume: f32,
    pub pitch: f32,
}

const STEP_FAMILY_OVERRIDES: [(&str, &str); 7] = [
    ("block.crop", "block.grass"),
    ("block.nether_wart", "block.stone"),
    ("block.small_amethyst_bud", "block.amethyst_cluster"),
    ("block.medium_amethyst_bud", "block.amethyst_cluster"),
    ("block.large_amethyst_bud", "block.amethyst_cluster"),
    ("block.bamboo_sapling", "block.bamboo"),
    ("block.lily_pad", "block.big_dripleaf"),
];

impl<'a> SoundType<'a> {
    /// `break_event` is a block's registered sound type, e.g. `minecraft:block.stone.break`.
    pub fn from_break_event(break_event: &'a str) -> Option<Self> {
        let family = break_event
            .strip_prefix("minecraft:")
            .unwrap_or(break_event)
            .strip_suffix(".break")?;
        let (volume, pitch) = match family {
            "block.metal" => (1.0, 1.5),
            "block.anvil" => (0.3, 1.0),
            _ => (1.0, 1.0),
        };
        Some(Self {
            family,
            volume,
            pitch,
        })
    }

    fn shared_family(&self) -> &str {
        STEP_FAMILY_OVERRIDES
            .iter()
            .find(|(own, _)| *own == self.family)
            .map_or(self.family, |(_, shared)| shared)
    }

    pub fn break_event(&self) -> String {
        format!("{}.break", self.family)
    }

    pub fn place_event(&self) -> String {
        format!("{}.place", self.family)
    }

    pub fn hit_event(&self) -> String {
        format!("{}.hit", self.shared_family())
    }

    pub fn step_event(&self) -> String {
        format!("{}.step", self.shared_family())
    }

    pub fn fall_event(&self) -> String {
        format!("{}.fall", self.shared_family())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stone_family_events() {
        let t = SoundType::from_break_event("minecraft:block.stone.break").unwrap();
        assert_eq!(t.step_event(), "block.stone.step");
        assert_eq!(t.fall_event(), "block.stone.fall");
        assert_eq!((t.volume, t.pitch), (1.0, 1.0));
    }

    #[test]
    fn crops_step_like_grass_and_metal_is_high_pitched() {
        let crop = SoundType::from_break_event("block.crop.break").unwrap();
        assert_eq!(crop.step_event(), "block.grass.step");
        assert_eq!(crop.break_event(), "block.crop.break");
        let metal = SoundType::from_break_event("block.metal.break").unwrap();
        assert_eq!(metal.pitch, 1.5);
    }

    #[test]
    fn non_break_names_are_rejected() {
        assert!(SoundType::from_break_event("").is_none());
        assert!(SoundType::from_break_event("block.stone.step").is_none());
    }
}
