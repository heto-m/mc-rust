use mcr_buf::{BlockPos, Decode, Encode, Result, read_varint, write_varint};

use super::{item::ItemStackTemplate, math::Vec3d};

pub const PARTICLE_TYPE_COUNT: i32 = 128;

macro_rules! kinds {
    ($(#[$m:meta])* $name:ident { $($variant:ident = $id:literal),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            pub const fn id(self) -> i32 {
                match self {
                    $(Self::$variant => $id),+
                }
            }

            pub const fn from_id(id: i32) -> Option<Self> {
                match id {
                    $($id => Some(Self::$variant),)+
                    _ => None,
                }
            }
        }
    };
}

kinds!(
    /// Particle types whose only option is a block state id.
    BlockParticleKind {
        Block = 1,
        BlockMarker = 2,
        FallingDust = 36,
        DustPillar = 121,
        BlockCrumble = 125,
    }
);

kinds!(
    /// Particle types whose only option is a packed ARGB colour.
    ColorParticleKind {
        EntityEffect = 28,
        TintedLeaves = 46,
        Flash = 52,
    }
);

kinds!(
    /// Particle types carrying a colour and a power.
    SpellParticleKind {
        Effect = 23,
        InstantEffect = 56,
    }
);

kinds!(
    /// Particle types carrying the number of water blocks.
    GeyserParticleKind {
        Geyser = 7,
        GeyserPlume = 10,
    }
);

kinds!(
    /// Particle types carrying water blocks and a burst impulse.
    GeyserBaseParticleKind {
        GeyserBase = 8,
        GeyserPoof = 9,
    }
);

pub const DRAGON_BREATH: i32 = 15;
pub const DUST: i32 = 21;
pub const DUST_COLOR_TRANSITION: i32 = 22;
pub const SCULK_CHARGE: i32 = 48;
pub const ITEM: i32 = 57;
pub const VIBRATION: i32 = 58;
pub const TRAIL: i32 = 59;
pub const SHRIEK: i32 = 115;

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
#[repr(u8)]
pub enum PositionSource {
    Block(BlockPos) = 0,
    Entity {
        #[mcr(varint)]
        entity_id: i32,
        y_offset: f32,
    } = 1,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Particle {
    Simple(i32),
    BlockState {
        kind: BlockParticleKind,
        state: i32,
    },
    Color {
        kind: ColorParticleKind,
        argb: i32,
    },
    Spell {
        kind: SpellParticleKind,
        color: i32,
        power: f32,
    },
    Geyser {
        kind: GeyserParticleKind,
        water_blocks: i32,
    },
    GeyserBase {
        kind: GeyserBaseParticleKind,
        water_blocks: i32,
        burst_impulse_base: f32,
    },
    DragonBreath {
        power: f32,
    },
    Dust {
        color: i32,
        scale: f32,
    },
    DustColorTransition {
        from: i32,
        to: i32,
        scale: f32,
    },
    SculkCharge {
        roll: f32,
    },
    Item(ItemStackTemplate),
    Vibration {
        source: PositionSource,
        arrival_ticks: i32,
    },
    Trail {
        target: Vec3d,
        color: i32,
        duration: i32,
    },
    Shriek {
        delay: i32,
    },
    /// A modded particle type's options, carried as their wire bytes.
    Modded {
        id: i32,
        data: Vec<u8>,
    },
}

impl Particle {
    pub fn type_id(&self) -> i32 {
        match self {
            Self::Simple(id) => *id,
            Self::BlockState { kind, .. } => kind.id(),
            Self::Color { kind, .. } => kind.id(),
            Self::Spell { kind, .. } => kind.id(),
            Self::Geyser { kind, .. } => kind.id(),
            Self::GeyserBase { kind, .. } => kind.id(),
            Self::DragonBreath { .. } => DRAGON_BREATH,
            Self::Dust { .. } => DUST,
            Self::DustColorTransition { .. } => DUST_COLOR_TRANSITION,
            Self::SculkCharge { .. } => SCULK_CHARGE,
            Self::Item(_) => ITEM,
            Self::Vibration { .. } => VIBRATION,
            Self::Trail { .. } => TRAIL,
            Self::Shriek { .. } => SHRIEK,
            Self::Modded { id, .. } => *id,
        }
    }
}

impl Encode for Particle {
    fn encode(&self, out: &mut Vec<u8>) {
        write_varint(out, self.type_id());
        match self {
            Self::Simple(_) => {}
            Self::BlockState { state, .. } => write_varint(out, *state),
            Self::Color { argb, .. } => argb.encode(out),
            Self::Spell { color, power, .. } => {
                color.encode(out);
                power.encode(out);
            }
            Self::Geyser { water_blocks, .. } => water_blocks.encode(out),
            Self::GeyserBase {
                water_blocks,
                burst_impulse_base,
                ..
            } => {
                water_blocks.encode(out);
                burst_impulse_base.encode(out);
            }
            Self::DragonBreath { power } => power.encode(out),
            Self::Dust { color, scale } => {
                color.encode(out);
                scale.encode(out);
            }
            Self::DustColorTransition { from, to, scale } => {
                from.encode(out);
                to.encode(out);
                scale.encode(out);
            }
            Self::SculkCharge { roll } => roll.encode(out),
            Self::Item(item) => item.encode(out),
            Self::Vibration {
                source,
                arrival_ticks,
            } => {
                source.encode(out);
                write_varint(out, *arrival_ticks);
            }
            Self::Trail {
                target,
                color,
                duration,
            } => {
                target.encode(out);
                color.encode(out);
                write_varint(out, *duration);
            }
            Self::Shriek { delay } => write_varint(out, *delay),
            Self::Modded { data, .. } => out.extend_from_slice(data),
        }
    }
}

impl Decode<'_> for Particle {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        let id = read_varint(input)?;
        Self::decode_options(id, input)
    }
}

impl Particle {
    /// The options that follow particle type `id`.
    pub(crate) fn decode_options(id: i32, input: &mut &[u8]) -> Result<Self> {
        if let Some(kind) = BlockParticleKind::from_id(id) {
            return Ok(Self::BlockState {
                kind,
                state: read_varint(input)?,
            });
        }
        if let Some(kind) = ColorParticleKind::from_id(id) {
            return Ok(Self::Color {
                kind,
                argb: i32::decode(input)?,
            });
        }
        if let Some(kind) = SpellParticleKind::from_id(id) {
            return Ok(Self::Spell {
                kind,
                color: i32::decode(input)?,
                power: f32::decode(input)?,
            });
        }
        if let Some(kind) = GeyserParticleKind::from_id(id) {
            return Ok(Self::Geyser {
                kind,
                water_blocks: i32::decode(input)?,
            });
        }
        if let Some(kind) = GeyserBaseParticleKind::from_id(id) {
            return Ok(Self::GeyserBase {
                kind,
                water_blocks: i32::decode(input)?,
                burst_impulse_base: f32::decode(input)?,
            });
        }
        Ok(match id {
            DRAGON_BREATH => Self::DragonBreath {
                power: f32::decode(input)?,
            },
            DUST => Self::Dust {
                color: i32::decode(input)?,
                scale: f32::decode(input)?,
            },
            DUST_COLOR_TRANSITION => Self::DustColorTransition {
                from: i32::decode(input)?,
                to: i32::decode(input)?,
                scale: f32::decode(input)?,
            },
            SCULK_CHARGE => Self::SculkCharge {
                roll: f32::decode(input)?,
            },
            ITEM => Self::Item(ItemStackTemplate::decode(input)?),
            VIBRATION => Self::Vibration {
                source: PositionSource::decode(input)?,
                arrival_ticks: read_varint(input)?,
            },
            TRAIL => Self::Trail {
                target: Vec3d::decode(input)?,
                color: i32::decode(input)?,
                duration: read_varint(input)?,
            },
            SHRIEK => Self::Shriek {
                delay: read_varint(input)?,
            },
            0..PARTICLE_TYPE_COUNT => Self::Simple(id),
            _ => Self::Modded {
                id,
                data: super::modded::skip_modded(|t| &t.particles, "particle type", id, input)?
                    .to_vec(),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::roundtrip;

    fn samples() -> Vec<Particle> {
        let mut all = vec![
            Particle::Simple(0),
            Particle::Simple(127),
            Particle::DragonBreath { power: 0.5 },
            Particle::Dust {
                color: 0xFF0000,
                scale: 1.0,
            },
            Particle::DustColorTransition {
                from: 1,
                to: -2,
                scale: 2.0,
            },
            Particle::SculkCharge { roll: 1.5 },
            Particle::Item(ItemStackTemplate {
                item: 800,
                count: 1,
                components: Default::default(),
            }),
            Particle::Vibration {
                source: PositionSource::Block(BlockPos::new(1, 2, 3)),
                arrival_ticks: 20,
            },
            Particle::Vibration {
                source: PositionSource::Entity {
                    entity_id: 300,
                    y_offset: 1.62,
                },
                arrival_ticks: 5,
            },
            Particle::Trail {
                target: Vec3d {
                    x: 1.0,
                    y: 2.0,
                    z: 3.0,
                },
                color: 0x00FF00,
                duration: 40,
            },
            Particle::Shriek { delay: 90 },
        ];
        for kind in [
            BlockParticleKind::Block,
            BlockParticleKind::BlockMarker,
            BlockParticleKind::FallingDust,
            BlockParticleKind::DustPillar,
            BlockParticleKind::BlockCrumble,
        ] {
            all.push(Particle::BlockState { kind, state: 1234 });
        }
        for kind in [
            ColorParticleKind::EntityEffect,
            ColorParticleKind::TintedLeaves,
            ColorParticleKind::Flash,
        ] {
            all.push(Particle::Color {
                kind,
                argb: 0x80FF_FFFFu32 as i32,
            });
        }
        for kind in [SpellParticleKind::Effect, SpellParticleKind::InstantEffect] {
            all.push(Particle::Spell {
                kind,
                color: 5,
                power: 0.25,
            });
        }
        for kind in [GeyserParticleKind::Geyser, GeyserParticleKind::GeyserPlume] {
            all.push(Particle::Geyser {
                kind,
                water_blocks: 3,
            });
        }
        for kind in [
            GeyserBaseParticleKind::GeyserBase,
            GeyserBaseParticleKind::GeyserPoof,
        ] {
            all.push(Particle::GeyserBase {
                kind,
                water_blocks: 3,
                burst_impulse_base: 0.5,
            });
        }
        all
    }

    #[test]
    fn roundtrip_every_variant() {
        for p in samples() {
            let bytes = roundtrip(&p);
            assert_eq!(mcr_buf::read_varint(&mut &bytes[..]).unwrap(), p.type_id());
        }
    }

    #[test]
    fn simple_is_only_the_id() {
        assert_eq!(roundtrip(&Particle::Simple(11)), [11]);
    }

    #[test]
    fn dust_bytes() {
        let bytes = roundtrip(&Particle::Dust {
            color: 0x00FF_0000,
            scale: 1.0,
        });
        assert_eq!(bytes, [21, 0, 0xFF, 0, 0, 0x3F, 0x80, 0, 0]);
    }

    #[test]
    fn block_state_is_varint() {
        assert_eq!(
            roundtrip(&Particle::BlockState {
                kind: BlockParticleKind::Block,
                state: 300,
            }),
            [1, 0xAC, 0x02]
        );
    }

    #[test]
    fn java_layout_fixtures() {
        let mut trail = vec![59, 0x3F, 0xF0, 0, 0, 0, 0, 0, 0];
        trail.extend([0; 8]);
        trail.extend([0xC0, 0, 0, 0, 0, 0, 0, 0]);
        trail.extend([0, 0, 0xFF, 0, 0xAC, 0x02]);
        let cases: Vec<(Particle, Vec<u8>)> = vec![
            (
                Particle::DustColorTransition {
                    from: 0x00FF_0000,
                    to: 0x0000_00FF,
                    scale: 0.5,
                },
                vec![22, 0, 0xFF, 0, 0, 0, 0, 0, 0xFF, 0x3F, 0, 0, 0],
            ),
            (
                Particle::Spell {
                    kind: SpellParticleKind::InstantEffect,
                    color: -1,
                    power: 1.0,
                },
                vec![56, 0xFF, 0xFF, 0xFF, 0xFF, 0x3F, 0x80, 0, 0],
            ),
            (
                Particle::Color {
                    kind: ColorParticleKind::Flash,
                    argb: 0x80FF_FFFFu32 as i32,
                },
                vec![52, 0x80, 0xFF, 0xFF, 0xFF],
            ),
            (
                Particle::Geyser {
                    kind: GeyserParticleKind::GeyserPlume,
                    water_blocks: 7,
                },
                vec![10, 0, 0, 0, 7],
            ),
            (
                Particle::GeyserBase {
                    kind: GeyserBaseParticleKind::GeyserPoof,
                    water_blocks: 3,
                    burst_impulse_base: 0.5,
                },
                vec![9, 0, 0, 0, 3, 0x3F, 0, 0, 0],
            ),
            (
                Particle::SculkCharge { roll: 1.0 },
                vec![48, 0x3F, 0x80, 0, 0],
            ),
            (
                Particle::Item(ItemStackTemplate {
                    item: 800,
                    count: 1,
                    components: Default::default(),
                }),
                vec![57, 0xA0, 0x06, 1, 0, 0],
            ),
            (
                Particle::Vibration {
                    source: PositionSource::Block(BlockPos::new(1, 0, 0)),
                    arrival_ticks: 20,
                },
                vec![58, 0, 0, 0, 0, 0x40, 0, 0, 0, 0, 20],
            ),
            (
                Particle::Vibration {
                    source: PositionSource::Entity {
                        entity_id: 300,
                        y_offset: 0.5,
                    },
                    arrival_ticks: 20,
                },
                vec![58, 1, 0xAC, 0x02, 0x3F, 0, 0, 0, 20],
            ),
            (
                Particle::Trail {
                    target: Vec3d {
                        x: 1.0,
                        y: 0.0,
                        z: -2.0,
                    },
                    color: 0x00FF00,
                    duration: 300,
                },
                trail,
            ),
            (Particle::Shriek { delay: 300 }, vec![115, 0xAC, 0x02]),
            (
                Particle::BlockState {
                    kind: BlockParticleKind::BlockCrumble,
                    state: 1,
                },
                vec![125, 1],
            ),
        ];
        for (particle, bytes) in cases {
            assert_eq!(roundtrip(&particle), bytes, "{particle:?}");
            assert_eq!(
                mcr_buf::from_bytes_exact::<Particle>(&bytes).unwrap(),
                particle
            );
        }
    }

    #[test]
    fn rejects_unknown_type() {
        assert!(Particle::decode(&mut &[0x80, 0x01][..]).is_err());
    }

    #[test]
    fn rejects_truncated_item_patch() {
        assert!(Particle::decode(&mut &[57, 1, 1, 1, 0][..]).is_err());
    }

    fn registry_ids() -> Option<Vec<(String, i32)>> {
        let path = std::env::current_dir().ok()?.ancestors().find_map(|dir| {
            let file = dir.join(".cache/versions/26.3/generated/reports/registries.json");
            file.exists().then_some(file)
        })?;
        let text = std::fs::read_to_string(path).ok()?;
        let section = &text[text.find("\"minecraft:particle_type\"")?..];
        let mut out = Vec::new();
        let mut name = None;
        for line in section.lines().skip(1) {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("\"minecraft:") {
                name = rest.split('"').next().map(str::to_owned);
            } else if let Some(rest) = line.strip_prefix("\"protocol_id\":") {
                let id = rest.trim().trim_end_matches(',').parse().ok()?;
                out.push((name.take()?, id));
                if out.len() == PARTICLE_TYPE_COUNT as usize {
                    break;
                }
            }
        }
        out.sort_by_key(|(_, id)| *id);
        Some(out)
    }

    #[test]
    fn matches_registry_order() {
        let Some(ids) = registry_ids() else { return };
        assert_eq!(ids.len(), PARTICLE_TYPE_COUNT as usize);
        assert!(ids.iter().enumerate().all(|(i, (_, id))| *id == i as i32));
        let id_of = |name: &str| ids.iter().find(|(n, _)| n == name).unwrap().1;
        let expected = [
            ("block", BlockParticleKind::Block.id()),
            ("block_marker", BlockParticleKind::BlockMarker.id()),
            ("falling_dust", BlockParticleKind::FallingDust.id()),
            ("dust_pillar", BlockParticleKind::DustPillar.id()),
            ("block_crumble", BlockParticleKind::BlockCrumble.id()),
            ("entity_effect", ColorParticleKind::EntityEffect.id()),
            ("tinted_leaves", ColorParticleKind::TintedLeaves.id()),
            ("flash", ColorParticleKind::Flash.id()),
            ("effect", SpellParticleKind::Effect.id()),
            ("instant_effect", SpellParticleKind::InstantEffect.id()),
            ("geyser", GeyserParticleKind::Geyser.id()),
            ("geyser_plume", GeyserParticleKind::GeyserPlume.id()),
            ("geyser_base", GeyserBaseParticleKind::GeyserBase.id()),
            ("geyser_poof", GeyserBaseParticleKind::GeyserPoof.id()),
            ("dragon_breath", DRAGON_BREATH),
            ("dust", DUST),
            ("dust_color_transition", DUST_COLOR_TRANSITION),
            ("sculk_charge", SCULK_CHARGE),
            ("item", ITEM),
            ("vibration", VIBRATION),
            ("trail", TRAIL),
            ("shriek", SHRIEK),
        ];
        for (name, id) in expected {
            assert_eq!(id_of(name), id, "{name}");
        }
    }
}
