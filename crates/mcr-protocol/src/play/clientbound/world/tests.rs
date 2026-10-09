use mcr_buf::{from_bytes_exact, to_bytes};
use mcr_nbt::Compound;
use mcr_world::{
    BlockEntityData as WorldBlockEntityData, ChunkData, ChunkSection, Heightmap, HeightmapKind,
    Heightmaps, LightData, LightNibbles, LightUpdate as WorldLightUpdate, PalettedContainer,
    WorldConfig,
};

use super::*;

fn decoded_like<'a, T: Decode<'a>>(_: &T, bytes: &'a [u8]) -> T {
    from_bytes_exact(bytes).unwrap()
}

macro_rules! rt {
    ($value:expr) => {{
        let value = $value;
        let bytes = to_bytes(&value);
        assert_eq!(decoded_like(&value, &bytes), value);
        bytes
    }};
}

#[test]
fn block_update_and_friends() {
    let pos = BlockPos::new(1, 0, 0);
    let pos_bytes = [0, 0, 0, 0x40, 0, 0, 0, 0];
    let bytes = rt!(BlockUpdate {
        pos,
        block_state: 5
    });
    assert_eq!(bytes, [&pos_bytes[..], &[5]].concat());
    let bytes = rt!(AddTransientBlock {
        pos,
        block_state: 300
    });
    assert_eq!(bytes, [&pos_bytes[..], &[0xac, 2]].concat());
    let bytes = rt!(BlockEvent {
        pos,
        b0: 1,
        b1: 2,
        block: 9
    });
    assert_eq!(bytes, [&pos_bytes[..], &[1, 2, 9]].concat());
    let bytes = rt!(BlockDestruction {
        entity_id: 3,
        pos,
        progress: 7
    });
    assert_eq!(bytes, [&[3][..], &pos_bytes, &[7]].concat());
    assert_eq!(rt!(BlockChangedAck { sequence: 200 }), [0xc8, 1]);
    assert_eq!(rt!(BundleDelimiter), Vec::<u8>::new());
    assert_eq!(rt!(ChunkBatchStart), Vec::<u8>::new());
    assert_eq!(rt!(ChunkBatchFinished { batch_size: 4 }), [4]);
    assert_eq!(
        rt!(GameTestHighlightPos {
            absolute_pos: pos,
            relative_pos: BlockPos::new(-1, -2, -3)
        }),
        [
            &pos_bytes[..],
            &[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xdf, 0xfe]
        ]
        .concat()
    );
}

#[test]
fn block_entity_data() {
    let mut tag = Compound::new();
    tag.insert("a", 1i32);
    let bytes = rt!(BlockEntityData {
        pos: BlockPos::new(0, 0, 0),
        block_entity_type: 2,
        tag: NetworkCompound(tag)
    });
    assert_eq!(&bytes[8..11], [2, 10, 3]);
}

#[test]
fn section_blocks_update_layout() {
    let packet = SectionBlocksUpdate {
        section: SectionPos { x: 1, y: -1, z: 0 },
        changes: vec![SectionBlockChange {
            x: 1,
            y: 2,
            z: 3,
            block_state: 5,
        }],
    };
    let bytes = rt!(packet);
    assert_eq!(
        bytes,
        [0, 0, 4, 0, 0, 0x0f, 0xff, 0xff, 1, 0xb2, 0xa2, 0x01]
    );
    rt!(SectionBlocksUpdate {
        section: SectionPos {
            x: -33_554,
            y: -2048,
            z: 2_000_000
        },
        changes: vec![],
    });
}

#[test]
fn borders_time_and_ticking() {
    rt!(InitializeBorder {
        new_center_x: 0.0,
        new_center_z: 1.0,
        old_size: 10.0,
        new_size: 20.0,
        lerp_time: 5_000_000_000,
        new_absolute_max_size: 29_999_984,
        warning_blocks: 5,
        warning_time: 15,
    });
    rt!(SetBorderCenter {
        new_center_x: 1.0,
        new_center_z: 2.0
    });
    assert_eq!(
        rt!(SetBorderLerpSize {
            old_size: 1.0,
            new_size: 2.0,
            lerp_time: 300
        }),
        [
            0x3f, 0xf0, 0, 0, 0, 0, 0, 0, 0x40, 0, 0, 0, 0, 0, 0, 0, 0xac, 0x02
        ]
    );
    rt!(SetBorderSize { size: 1.0 });
    assert_eq!(rt!(SetBorderWarningDelay { warning_delay: 15 }), [15]);
    assert_eq!(rt!(SetBorderWarningDistance { warning_blocks: 5 }), [5]);
    assert_eq!(rt!(SetCamera { camera_id: 1 }), [1]);
    assert_eq!(
        rt!(SetChunkCacheCenter { x: -1, z: 2 }),
        [0xff, 0xff, 0xff, 0xff, 0x0f, 2]
    );
    assert_eq!(rt!(SetChunkCacheRadius { radius: 8 }), [8]);
    assert_eq!(
        rt!(SetSimulationDistance {
            simulation_distance: 8
        }),
        [8]
    );
    assert_eq!(rt!(TickingStep { tick_steps: 3 }), [3]);
    assert_eq!(
        rt!(TickingState {
            tick_rate: 20.0,
            is_frozen: true
        }),
        [0x41, 0xa0, 0, 0, 1]
    );
    let bytes = rt!(SetTime {
        game_time: 1,
        clock_updates: vec![ClockUpdate {
            clock: 0,
            total_ticks: 300,
            partial_tick: 0.0,
            rate: 1.0,
        }],
    });
    assert_eq!(&bytes[8..13], [1, 0, 0xac, 2, 0]);
    let bytes = rt!(SetDefaultSpawnPosition {
        dimension: Identifier::parse("overworld").unwrap(),
        pos: BlockPos::new(0, 64, 0),
        yaw: 0.0,
        pitch: 0.0,
    });
    assert_eq!(bytes[0] as usize, "minecraft:overworld".len());
}

#[test]
fn events_and_sounds() {
    let bytes = rt!(LevelEvent {
        event: 1010,
        pos: BlockPos::new(0, 0, 0),
        data: 2,
        global: true
    });
    assert_eq!(&bytes[..4], [0, 0, 3, 0xf2]);
    assert_eq!(
        rt!(GameEvent {
            event: 7,
            param: 1.0
        }),
        [7, 0x3f, 0x80, 0, 0]
    );
    assert_eq!(
        rt!(DebugSample {
            sample: vec![1, -2],
            sample_type: DebugSampleType::TickTime
        }),
        [
            2, 0, 0, 0, 0, 0, 0, 0, 1, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xfe, 0
        ]
    );
    assert_eq!(
        rt!(ForgetLevelChunk {
            pos: ChunkPos { x: -1, z: 2 }
        }),
        [0, 0, 0, 2, 0xff, 0xff, 0xff, 0xff]
    );
}

#[test]
fn stop_sound_flags() {
    assert_eq!(
        rt!(StopSound {
            source: None,
            name: None
        }),
        [0]
    );
    assert_eq!(
        rt!(StopSound {
            source: Some(SoundSource::Players),
            name: None
        }),
        [1, 7]
    );
    let bytes = rt!(StopSound {
        source: Some(SoundSource::Ui),
        name: Some(Identifier::parse("a:b").unwrap()),
    });
    assert_eq!(bytes, [3, 10, 3, b'a', b':', b'b']);
    assert_eq!(
        rt!(StopSound {
            source: None,
            name: Some(Identifier::parse("a:b").unwrap())
        })[0],
        2
    );
}

#[test]
fn test_instance_block_status_bytes() {
    assert_eq!(
        rt!(TestInstanceBlockStatus {
            status: Component::text("ok"),
            size: Some(Vec3i { x: 1, y: -1, z: 3 })
        }),
        [8, 0, 2, b'o', b'k', 1, 1, 0xff, 0xff, 0xff, 0xff, 0x0f, 3]
    );
    assert_eq!(
        rt!(TestInstanceBlockStatus {
            status: Component::text("ok"),
            size: None
        }),
        [8, 0, 2, b'o', b'k', 0]
    );
}

fn cfg() -> WorldConfig {
    WorldConfig::new(35723, 80, -64, 384).unwrap()
}

fn sample_chunk(cfg: &WorldConfig) -> ChunkData {
    let sections = (0..cfg.section_count())
        .map(|s| {
            let mut sec = ChunkSection::empty(cfg);
            sec.set_block(1, 2, 3, mcr_world::BlockState(s as u16 + 1), cfg);
            sec
        })
        .collect();
    let mut heightmaps = Heightmaps::default();
    heightmaps.set(HeightmapKind::MotionBlocking, Heightmap::empty(cfg));
    ChunkData {
        heightmaps,
        sections,
        block_entities: vec![WorldBlockEntityData {
            packed_xz: 0x35,
            y: -12,
            type_id: 7,
            nbt: vec![10, 0].into(),
        }],
    }
}

fn sample_light() -> LightData {
    let mut nibbles = LightNibbles::default();
    nibbles.set(1, 0, 0, 9);
    LightData {
        sky: vec![
            WorldLightUpdate {
                section: 3,
                data: Some(nibbles),
            },
            WorldLightUpdate {
                section: 25,
                data: None,
            },
        ],
        block: vec![],
    }
}

#[test]
fn level_chunk_payload_feeds_world_decoders() {
    let cfg = cfg();
    let (chunk, light) = (sample_chunk(&cfg), sample_light());
    let mut payload = Vec::new();
    chunk.encode(&mut payload);
    light.encode(&mut payload);

    let packet = LevelChunkWithLight {
        x: -3,
        z: 4,
        payload: RestBytes(&payload),
    };
    let bytes = to_bytes(&packet);
    assert_eq!(&bytes[..8], [0xff, 0xff, 0xff, 0xfd, 0, 0, 0, 4]);
    let decoded = from_bytes_exact::<LevelChunkWithLight>(&bytes).unwrap();
    assert_eq!(decoded, packet);

    let mut input = decoded.payload.0;
    assert_eq!(ChunkData::decode(&mut input, &cfg).unwrap(), chunk);
    let back = LightData::decode(&mut input, &cfg).unwrap();
    assert!(input.is_empty());
    assert_eq!(back.sky.len(), 2);
}

#[test]
fn light_update_payload_feeds_world_decoder() {
    let cfg = cfg();
    let mut payload = Vec::new();
    sample_light().encode(&mut payload);
    let packet = LightUpdate {
        x: -1,
        z: 300,
        payload: RestBytes(&payload),
    };
    let bytes = to_bytes(&packet);
    assert_eq!(&bytes[..6], [0xff, 0xff, 0xff, 0xff, 0x0f, 0xac]);
    let decoded = from_bytes_exact::<LightUpdate>(&bytes).unwrap();
    let mut input = decoded.payload.0;
    LightData::decode(&mut input, &cfg).unwrap();
    assert!(input.is_empty());
}

#[test]
fn chunks_biomes_bytes_and_limit() {
    let bytes = [1, 0xff, 0xff, 0xff, 0xff, 0, 0, 0, 1, 3, 7, 8, 9];
    let packet = ChunksBiomes {
        chunks: vec![ChunkBiomeData {
            pos: ChunkPos { x: 1, z: -1 },
            buffer: ByteArray(&[7, 8, 9]),
        }],
    };
    assert_eq!(from_bytes_exact::<ChunksBiomes>(&bytes).unwrap(), packet);
    assert_eq!(to_bytes(&packet), bytes);

    let mut oversized = bytes[..9].to_vec();
    oversized.extend([0x81, 0x80, 0x80, 0x01]);
    oversized.resize(oversized.len() + 2_097_153, 0);
    assert!(matches!(
        from_bytes_exact::<ChunksBiomes>(&oversized),
        Err(DecodeError::TooLong { .. })
    ));
}

#[test]
fn biome_buffers_feed_world_decoder() {
    let cfg = cfg();
    let chunk = sample_chunk(&cfg);
    let mut buffer = Vec::new();
    for section in &chunk.sections {
        section.biomes().encode(&mut buffer);
    }
    let packet = ChunksBiomes {
        chunks: vec![ChunkBiomeData {
            pos: ChunkPos { x: 1, z: -1 },
            buffer: ByteArray(&buffer),
        }],
    };
    let bytes = to_bytes(&packet);
    let decoded = from_bytes_exact::<ChunksBiomes>(&bytes).unwrap();
    assert_eq!(decoded, packet);
    let mut input = decoded.chunks[0].buffer.0;
    for _ in 0..cfg.section_count() {
        PalettedContainer::<64>::decode(&mut input, cfg.biome_limits()).unwrap();
    }
    assert!(input.is_empty());
}

#[test]
fn explode() {
    rt!(Explode {
        center: Vec3d::default(),
        radius: 4.0,
        block_count: 3,
        player_knockback: None,
        particle: Particle::Simple(0),
        sound: IdOr::Id(3),
        block_particles: vec![ExplosionParticle {
            particle: Particle::Simple(0),
            scaling: 1.0,
            speed: 1.0,
            weight: 2
        }],
        play_sound: true,
    });
}

#[test]
fn level_particles() {
    rt!(LevelParticles {
        particle: Particle::Simple(0),
        override_limiter: false,
        always_show: true,
        x: 1.0,
        y: 2.0,
        z: 3.0,
        x_dist: 0.0,
        y_dist: 0.0,
        z_dist: 0.0,
        x_max_speed: 0.0,
        y_max_speed: 0.0,
        z_max_speed: 0.0,
        count: 5,
        randomization: ParticleRandomization::Alternative,
    });
}

#[test]
fn sounds() {
    rt!(Sound {
        sound: IdOr::Id(3),
        source: SoundSource::Blocks,
        x: 1,
        y: 2,
        z: 3,
        volume: 1.0,
        pitch: 1.0,
        seed: 9
    });
    assert_eq!(
        rt!(SoundEntity {
            sound: IdOr::Id(3),
            source: SoundSource::Blocks,
            entity_id: 300,
            volume: 1.0,
            pitch: 0.5,
            seed: 9
        }),
        [
            4, 4, 0xac, 0x02, 0x3f, 0x80, 0, 0, 0x3f, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 9
        ]
    );
    assert_eq!(
        rt!(SoundEntity {
            sound: IdOr::Inline(SoundEvent {
                location: Identifier::parse("a:b").unwrap(),
                fixed_range: Some(16.0),
            }),
            source: SoundSource::Ui,
            entity_id: 1,
            volume: 1.0,
            pitch: 1.0,
            seed: -1
        }),
        [
            0, 3, b'a', b':', b'b', 1, 0x41, 0x80, 0, 0, 10, 1, 0x3f, 0x80, 0, 0, 0x3f, 0x80, 0, 0,
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff
        ]
    );
}
