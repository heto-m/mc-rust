//! Inbound translation cost per version on representative play frames, built from each version's tables.

use std::hint::black_box;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use mcr_buf::write_varint;
use mcr_protocol::dispatch::PlayClientbound;
use mcr_protocol::types::component::COMPONENT_NAMES;
use mcr_protocol::version::tables::VersionTables;
use mcr_protocol::version::{Session, Translated, Wire};
use mcr_protocol::{Direction, PacketIds, ProtocolState};

struct Old {
    tables: VersionTables,
    /// 1.21.11: sections without a fluid count, stacks where 26.1 has templates.
    stacks: bool,
}

impl Old {
    fn back(&self, key: &str, canonical: i32) -> i32 {
        self.tables
            .map(key)
            .map_or(Some(canonical), |m| m.back(canonical))
            .unwrap_or_else(|| panic!("{key}: no old id for {canonical}"))
    }

    fn state(&self, block: &str) -> i32 {
        let data = mcr_data::v26_3();
        let b = data.block_by_name(block).unwrap();
        self.back(
            "block_state",
            data.block_info(b).unwrap().default_state as i32,
        )
    }

    fn item(&self, name: &str) -> i32 {
        let id = mcr_data::v26_3().item_by_name(name).unwrap() as i32;
        self.back("registry/minecraft:item", id)
    }

    fn registry(&self, registry: &str, name: &str) -> i32 {
        let id = mcr_data::v26_3()
            .registry(registry)
            .unwrap()
            .id_of(name)
            .unwrap() as i32;
        self.back(&VersionTables::registry_key(registry), id)
    }

    fn component(&self, name: &str) -> i32 {
        let id = COMPONENT_NAMES.iter().position(|n| *n == name).unwrap() as i32;
        self.back("registry/minecraft:data_component_type", id)
    }
}

fn varint(out: &mut Vec<u8>, v: i32) {
    write_varint(out, v);
}

/// A full-height chunk: 24 four-bit sections, heightmaps, four chests and sky light.
fn chunk(old: &Old) -> Vec<u8> {
    let mut p = vec![0, 0, 0, 3, 0, 0, 0, 4];
    varint(&mut p, 3);
    for kind in [1, 4, 5] {
        varint(&mut p, kind);
        varint(&mut p, 37);
        p.extend(std::iter::repeat_n(0x11, 37 * 8));
    }
    let palette = [
        "minecraft:stone",
        "minecraft:dirt",
        "minecraft:grass_block",
        "minecraft:deepslate",
        "minecraft:water",
        "minecraft:air",
    ]
    .map(|b| old.state(b));
    let mut sections = Vec::new();
    let mut seed = 0x9e37_79b9_7f4a_7c15u64;
    for _ in 0..24 {
        if old.stacks {
            sections.extend([0x10, 0, 4]);
        } else {
            sections.extend([0x10, 0, 0, 0, 4]);
        }
        varint(&mut sections, palette.len() as i32);
        for &s in &palette {
            varint(&mut sections, s);
        }
        for _ in 0..256 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let mut word = 0u64;
            for k in 0..16 {
                word |= ((seed >> (k * 4)) % 6) << (k * 4);
            }
            sections.extend(word.to_be_bytes());
        }
        sections.extend([0, 0]);
    }
    varint(&mut p, sections.len() as i32);
    p.extend(&sections);
    let chest = old.registry("minecraft:block_entity_type", "minecraft:chest");
    varint(&mut p, 4);
    for i in 0..4u8 {
        p.extend([i, 0, 64]);
        varint(&mut p, chest);
        p.extend([10, 8, 0, 4, b'L', b'o', b'c', b'k', 0, 0, 0]);
    }
    for mask in [0x01ff_ffffu64, 0, 0, 0] {
        varint(&mut p, 1);
        p.extend(mask.to_be_bytes());
    }
    varint(&mut p, 25);
    for _ in 0..25 {
        varint(&mut p, 2048);
        p.extend([0xff; 2048]);
    }
    varint(&mut p, 0);
    p
}

/// A player inventory: stone stacks and named, damaged swords.
fn inventory(old: &Old) -> Vec<u8> {
    let mut p = Vec::new();
    varint(&mut p, 0);
    varint(&mut p, 1);
    varint(&mut p, 46);
    let name = mcr_buf::to_bytes(&mcr_protocol::types::Component::text("Blade"));
    for slot in 0..46 {
        match slot % 3 {
            0 => p.push(0),
            1 => {
                varint(&mut p, 64);
                varint(&mut p, old.item("minecraft:stone"));
                p.extend([0, 0]);
            }
            _ => {
                varint(&mut p, 1);
                varint(&mut p, old.item("minecraft:diamond_sword"));
                p.extend([3, 0]);
                varint(&mut p, old.component("damage"));
                varint(&mut p, 17);
                varint(&mut p, old.component("custom_name"));
                p.extend(&name);
                varint(&mut p, old.component("repair_cost"));
                varint(&mut p, 2);
            }
        }
    }
    p.push(0);
    p
}

/// A shulker box with 27 stacks of stone in its `container` component.
fn shulker(old: &Old) -> Vec<u8> {
    let mut p = Vec::new();
    varint(&mut p, 0);
    varint(&mut p, 1);
    p.extend([0, 5]);
    varint(&mut p, 1);
    varint(&mut p, old.item("minecraft:shulker_box"));
    p.extend([1, 0]);
    varint(&mut p, old.component("container"));
    varint(&mut p, 27);
    for _ in 0..27 {
        if old.stacks {
            varint(&mut p, 64);
            varint(&mut p, old.item("minecraft:stone"));
        } else {
            p.push(1);
            varint(&mut p, old.item("minecraft:stone"));
            varint(&mut p, 64);
        }
        p.extend([0, 0]);
    }
    p
}

/// A sheep's flags, air, baby flag and wool.
fn entity_data() -> Vec<u8> {
    let mut p = vec![7];
    p.extend([0, 0, 0, 1, 1, 0x80, 0x0a, 16, 8, 0, 17, 0, 3, 0xff]);
    p
}

fn frames(wire: Wire, old: &Old) -> Vec<(&'static str, i32, Vec<u8>)> {
    let ids = PacketIds::v26_3();
    let id = |name: &str| {
        let canonical = ids
            .id_by_name(ProtocolState::Play, Direction::Clientbound, name)
            .unwrap();
        wire.wire_clientbound_id(ProtocolState::Play, canonical)
            .unwrap()
    };
    vec![
        ("chunk", id("level_chunk_with_light"), chunk(old)),
        ("inventory", id("container_set_content"), inventory(old)),
        ("shulker box", id("container_set_slot"), shulker(old)),
        ("entity data", id("set_entity_data"), entity_data()),
        ("move", id("move_entity_pos"), vec![9, 0, 1, 0, 2, 0, 3, 1]),
        ("keep alive", id("keep_alive"), vec![0; 8]),
    ]
}

fn translate(c: &mut Criterion) {
    let ids = PacketIds::v26_3();
    let versions: [(&str, i32, &[u8]); 3] = [
        (
            "26.2",
            776,
            include_bytes!("../src/version/v26_2/tables.bin"),
        ),
        (
            "26.1.2",
            775,
            include_bytes!("../src/version/v26_1_2/tables.bin"),
        ),
        (
            "1.21.11",
            774,
            include_bytes!("../src/version/v1_21_11/tables.bin"),
        ),
    ];
    for (name, protocol, tables) in versions {
        let wire = Wire::any_for_protocol(protocol).unwrap();
        let old = Old {
            tables: VersionTables::decode(tables).unwrap(),
            stacks: protocol < 775,
        };
        let mut group = c.benchmark_group(format!("translate {name}"));
        for (what, id, payload) in frames(wire, &old) {
            let (mut session, mut out) = (Session::default(), Vec::new());
            let Translated::Packet(new_id, bytes) = wire
                .inbound(ProtocolState::Play, id, &payload, &mut session, &mut out)
                .unwrap()
            else {
                panic!("{what} did not translate to one packet")
            };
            PlayClientbound::decode(new_id, bytes, ids).unwrap();
            let canonical = bytes.to_vec();
            group.throughput(Throughput::Bytes(payload.len() as u64));
            group.bench_function(what, |b| {
                b.iter(|| {
                    black_box(
                        wire.inbound(
                            ProtocolState::Play,
                            id,
                            black_box(&payload),
                            &mut session,
                            &mut out,
                        )
                        .unwrap(),
                    );
                });
            });
            group.bench_function(format!("{what}: decode translated"), |b| {
                b.iter(|| black_box(PlayClientbound::decode(new_id, &canonical, ids).unwrap()));
            });
        }
        group.finish();
    }
}

criterion_group!(benches, translate);
criterion_main!(benches);
