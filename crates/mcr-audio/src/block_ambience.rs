//! Random ambient sounds of blocks and fluids near the player (vanilla `animateTick`, sound part).

use crate::category::SoundSource;
use crate::rng::FastRandom;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmbientBlock {
    Campfire,
    Fire,
    Furnace,
    BlastFurnace,
    Smoker,
    NetherPortal,
    RespawnAnchor,
    BubbleColumn { drag_down: bool },
    Eyeblossom,
    DriedGhast { waterlogged: bool },
    FireflyBush,
    PotentSulfur,
    HangingMoss,
    Candle { count: u8 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmbientFluid {
    FlowingWater,
    Lava,
}

/// World questions some cues depend on; every answer is for the block position given.
pub trait AmbientProbe {
    fn pale_moss_below(&self, pos: [i32; 3]) -> bool;
    fn pale_wood_above(&self, pos: [i32; 3]) -> bool;
    fn ghast_trigger_below(&self, pos: [i32; 3]) -> bool;
    fn exposed_to_sky(&self, pos: [i32; 3]) -> bool;
    fn firefly_sounds_here(&self, pos: [i32; 3]) -> bool;
    fn air_above(&self, pos: [i32; 3]) -> bool;
    fn water_source_above(&self, pos: [i32; 3]) -> bool;
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cue {
    pub event: &'static str,
    pub pos: [f64; 3],
    pub volume: f32,
    pub pitch: f32,
    pub category: SoundSource,
}

fn cue(
    event: &'static str,
    pos: [f64; 3],
    volume: f32,
    pitch: f32,
    category: SoundSource,
) -> Option<Cue> {
    Some(Cue {
        event,
        pos,
        volume,
        pitch,
        category,
    })
}

fn center(pos: [i32; 3]) -> [f64; 3] {
    pos.map(|v| f64::from(v) + 0.5)
}

fn corner(pos: [i32; 3]) -> [f64; 3] {
    pos.map(f64::from)
}

pub fn block_cue(
    kind: AmbientBlock,
    pos: [i32; 3],
    rng: &mut FastRandom,
    probe: &dyn AmbientProbe,
) -> Option<Cue> {
    use SoundSource::{Ambient, Blocks};
    match kind {
        AmbientBlock::Campfire => {
            if rng.below(10) != 0 {
                return None;
            }
            cue(
                "block.campfire.crackle",
                center(pos),
                0.5 + rng.f32(),
                rng.f32() * 0.7 + 0.6,
                Blocks,
            )
        }
        AmbientBlock::Fire => {
            if rng.below(24) != 0 {
                return None;
            }
            cue(
                "block.fire.ambient",
                center(pos),
                1.0 + rng.f32(),
                rng.f32() * 0.7 + 0.3,
                Blocks,
            )
        }
        AmbientBlock::Furnace => furnace("block.furnace.fire_crackle", pos, rng),
        AmbientBlock::BlastFurnace => furnace("block.blastfurnace.fire_crackle", pos, rng),
        AmbientBlock::Smoker => furnace("block.smoker.smoke", pos, rng),
        AmbientBlock::NetherPortal => {
            if rng.below(100) != 0 {
                return None;
            }
            cue(
                "block.portal.ambient",
                center(pos),
                0.5,
                rng.f32() * 0.4 + 0.8,
                Blocks,
            )
        }
        AmbientBlock::RespawnAnchor => {
            if rng.below(100) != 0 {
                return None;
            }
            cue(
                "block.respawn_anchor.ambient",
                center(pos),
                1.0,
                1.0,
                Blocks,
            )
        }
        AmbientBlock::BubbleColumn { drag_down } => {
            if rng.below(200) != 0 {
                return None;
            }
            let event = if drag_down {
                "block.bubble_column.whirlpool_ambient"
            } else {
                "block.bubble_column.upwards_ambient"
            };
            cue(
                event,
                corner(pos),
                0.2 + rng.f32() * 0.2,
                0.9 + rng.f32() * 0.15,
                Blocks,
            )
        }
        AmbientBlock::Eyeblossom => {
            if rng.below(700) != 0 || !probe.pale_moss_below(pos) {
                return None;
            }
            cue("block.eyeblossom.idle", corner(pos), 1.0, 1.0, Ambient)
        }
        AmbientBlock::DriedGhast { waterlogged } => {
            if rng.below(40) != 0 {
                return None;
            }
            if waterlogged {
                cue(
                    "block.dried_ghast.ambient_water",
                    center(pos),
                    1.0,
                    1.0,
                    Blocks,
                )
            } else if probe.ghast_trigger_below(pos) {
                cue("block.dried_ghast.ambient", center(pos), 1.0, 1.0, Blocks)
            } else {
                None
            }
        }
        AmbientBlock::FireflyBush => {
            if rng.below(30) != 0 || !probe.firefly_sounds_here(pos) || !probe.exposed_to_sky(pos) {
                return None;
            }
            cue("block.firefly_bush.idle", center(pos), 1.0, 1.0, Ambient)
        }
        AmbientBlock::PotentSulfur => {
            if !probe.water_source_above(pos) || rng.below(10) != 0 {
                return None;
            }
            cue(
                "block.potent_sulfur.noxious_gas",
                corner(pos),
                1.0,
                1.0,
                Ambient,
            )
        }
        AmbientBlock::HangingMoss => {
            if rng.below(500) != 0 || !probe.pale_wood_above(pos) {
                return None;
            }
            cue(
                "block.pale_hanging_moss.idle",
                corner(pos),
                1.0,
                1.0,
                Ambient,
            )
        }
        AmbientBlock::Candle { count } => {
            for _ in 0..count.clamp(1, 4) {
                if rng.f32() < 0.17 {
                    return cue(
                        "block.candle.ambient",
                        center(pos),
                        1.0 + rng.f32(),
                        rng.f32() * 0.7 + 0.3,
                        Blocks,
                    );
                }
            }
            None
        }
    }
}

fn furnace(event: &'static str, pos: [i32; 3], rng: &mut FastRandom) -> Option<Cue> {
    if rng.f64() >= 0.1 {
        return None;
    }
    cue(
        event,
        [
            f64::from(pos[0]) + 0.5,
            f64::from(pos[1]),
            f64::from(pos[2]) + 0.5,
        ],
        1.0,
        1.0,
        SoundSource::Blocks,
    )
}

pub fn fluid_cue(
    kind: AmbientFluid,
    pos: [i32; 3],
    rng: &mut FastRandom,
    probe: &dyn AmbientProbe,
) -> Option<Cue> {
    match kind {
        AmbientFluid::FlowingWater => {
            if rng.below(64) != 0 {
                return None;
            }
            cue(
                "block.water.ambient",
                center(pos),
                rng.f32() * 0.25 + 0.75,
                rng.f32() + 0.5,
                SoundSource::Ambient,
            )
        }
        AmbientFluid::Lava => {
            if !probe.air_above(pos) {
                return None;
            }
            if rng.below(100) == 0 {
                let at = [
                    f64::from(pos[0]) + rng.f64(),
                    f64::from(pos[1]) + 1.0,
                    f64::from(pos[2]) + rng.f64(),
                ];
                return cue(
                    "block.lava.pop",
                    at,
                    0.2 + rng.f32() * 0.2,
                    0.9 + rng.f32() * 0.15,
                    SoundSource::Ambient,
                );
            }
            if rng.below(200) == 0 {
                return cue(
                    "block.lava.ambient",
                    corner(pos),
                    0.2 + rng.f32() * 0.2,
                    0.9 + rng.f32() * 0.15,
                    SoundSource::Ambient,
                );
            }
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Probe(bool);

    impl AmbientProbe for Probe {
        fn pale_moss_below(&self, _: [i32; 3]) -> bool {
            self.0
        }
        fn pale_wood_above(&self, _: [i32; 3]) -> bool {
            self.0
        }
        fn ghast_trigger_below(&self, _: [i32; 3]) -> bool {
            self.0
        }
        fn exposed_to_sky(&self, _: [i32; 3]) -> bool {
            self.0
        }
        fn firefly_sounds_here(&self, _: [i32; 3]) -> bool {
            self.0
        }
        fn air_above(&self, _: [i32; 3]) -> bool {
            self.0
        }
        fn water_source_above(&self, _: [i32; 3]) -> bool {
            self.0
        }
    }

    fn rate(kind: AmbientBlock, probe: bool) -> f64 {
        let mut rng = FastRandom::new(3);
        let hits = (0..200_000)
            .filter(|_| block_cue(kind, [0, 0, 0], &mut rng, &Probe(probe)).is_some())
            .count();
        hits as f64 / 200_000.0
    }

    #[test]
    fn fire_and_portal_rates_match_vanilla() {
        assert!((rate(AmbientBlock::Fire, true) - 1.0 / 24.0).abs() < 0.004);
        assert!((rate(AmbientBlock::NetherPortal, true) - 0.01).abs() < 0.002);
        assert!((rate(AmbientBlock::Campfire, true) - 0.1).abs() < 0.005);
        assert!((rate(AmbientBlock::Furnace, true) - 0.1).abs() < 0.005);
    }

    #[test]
    fn conditional_cues_need_their_context() {
        assert_eq!(rate(AmbientBlock::Eyeblossom, false), 0.0);
        assert!(rate(AmbientBlock::Eyeblossom, true) > 0.0);
        assert_eq!(rate(AmbientBlock::HangingMoss, false), 0.0);
        assert_eq!(rate(AmbientBlock::FireflyBush, false), 0.0);
        assert_eq!(rate(AmbientBlock::PotentSulfur, false), 0.0);
        assert_eq!(
            rate(AmbientBlock::DriedGhast { waterlogged: false }, false),
            0.0
        );
        assert!(rate(AmbientBlock::DriedGhast { waterlogged: true }, false) > 0.0);
    }

    #[test]
    fn lava_pops_only_below_open_air() {
        let mut rng = FastRandom::new(5);
        let mut pops = 0;
        for _ in 0..100_000 {
            assert!(fluid_cue(AmbientFluid::Lava, [0, 0, 0], &mut rng, &Probe(false)).is_none());
            if let Some(c) = fluid_cue(AmbientFluid::Lava, [4, 10, 4], &mut rng, &Probe(true)) {
                assert!(matches!(c.event, "block.lava.pop" | "block.lava.ambient"));
                assert_eq!(c.category, SoundSource::Ambient);
                if c.event == "block.lava.pop" {
                    assert_eq!(c.pos[1], 11.0);
                    pops += 1;
                }
            }
        }
        assert!((800..1200).contains(&pops), "{pops}");
    }

    #[test]
    fn bubble_column_picks_direction() {
        let mut rng = FastRandom::new(1);
        for _ in 0..10_000 {
            if let Some(c) = block_cue(
                AmbientBlock::BubbleColumn { drag_down: true },
                [0; 3],
                &mut rng,
                &Probe(true),
            ) {
                assert_eq!(c.event, "block.bubble_column.whirlpool_ambient");
            }
        }
    }
}
