mod common;

use common::*;
use mcr_block_entity::mcr_entity_model::glam::{DVec3, Vec3};
use mcr_block_entity::{
    BannerData, BannerLayer, BeaconData, BeamSection, BlockEntityData, BlockEntityKind as K,
    CampfireData, ConduitData, DyeColor, GatewayData, ItemRef, PotData, SignData, SignSide,
    SkullData, SkullProfile, SpawnerData, VaultData,
};
use mcr_protocol::types::text::Component;

const W: u32 = 1600;
const H: u32 = 800;

fn dye(name: &str) -> DyeColor {
    DyeColor::from_name(name).unwrap()
}

fn row(h: &mut Harness, name: &str, cam: DVec3, target: Vec3, fov: f32, ticks: i64) {
    let frame = h.frame(cam, target, fov, W, H);
    h.advance(ticks, &frame);
    let shot = h.render(&frame, W, H);
    println!("{}", shot.save(name).display());
    assert!(shot.covered() > 2000, "{name} drew nothing");
}

#[test]
fn containers() {
    let Some(mut h) = Harness::new() else { return };
    let none = BlockEntityData::None;
    let s = &mut h.scene;
    s.set(
        [0, 0, 0],
        state("chest", &[("facing", "south"), ("type", "left")]),
        K::Chest,
        none.clone(),
    );
    s.set(
        [1, 0, 0],
        state("chest", &[("facing", "south"), ("type", "right")]),
        K::Chest,
        none.clone(),
    );
    s.set(
        [3, 0, 0],
        state("trapped_chest", &[("facing", "south")]),
        K::TrappedChest,
        none.clone(),
    );
    s.set(
        [5, 0, 0],
        state("ender_chest", &[("facing", "south")]),
        K::EnderChest,
        none.clone(),
    );
    for (i, ox) in [
        "copper_chest",
        "exposed_copper_chest",
        "weathered_copper_chest",
        "oxidized_copper_chest",
    ]
    .iter()
    .enumerate()
    {
        s.set(
            [i as i32 * 2, 0, 3],
            state(ox, &[("facing", "south")]),
            K::Chest,
            none.clone(),
        );
    }
    s.set(
        [7, 0, 0],
        state("chest", &[("facing", "west")]),
        K::Chest,
        none.clone(),
    );
    for (i, c) in [
        "white", "red", "blue", "lime", "purple", "black", "orange", "cyan",
    ]
    .iter()
    .enumerate()
    {
        let name = format!("{c}_shulker_box");
        s.set(
            [i as i32 * 2, 0, -4],
            state(&name, &[("facing", "up")]),
            K::ShulkerBox,
            none.clone(),
        );
    }
    s.set(
        [9, 0, -4],
        state("shulker_box", &[("facing", "north")]),
        K::ShulkerBox,
        none.clone(),
    );
    s.set(
        [11, 0, -4],
        state("blue_shulker_box", &[("facing", "east")]),
        K::ShulkerBox,
        none.clone(),
    );
    s.block_event([0, 0, 0], 1, 1, 0);
    s.block_event([1, 0, 0], 1, 1, 0);
    s.block_event([7, 0, 0], 1, 1, 0);
    s.block_event([2, 0, -4], 1, 1, 0);
    s.block_event([6, 0, -4], 1, 1, 0);
    s.block_event([11, 0, -4], 1, 1, 0);
    // half open double chest: stop opening after 5 ticks
    let frame = h.frame(
        DVec3::new(6.0, 5.0, 12.0),
        Vec3::new(5.5, 0.3, -0.5),
        45.0,
        W,
        H,
    );
    h.advance(5, &frame);
    h.scene.block_event([0, 0, 0], 1, 0, 0);
    h.scene.block_event([1, 0, 0], 1, 0, 0);
    h.scene.block_event([2, 0, -4], 1, 0, 0);
    row(
        &mut h,
        "containers",
        DVec3::new(6.0, 5.0, 12.0),
        Vec3::new(5.5, 0.3, -0.5),
        45.0,
        0,
    );
}

#[test]
fn banners_and_heads() {
    let Some(mut h) = Harness::new() else { return };
    let patterns = [
        ("stripe_top", "red"),
        ("circle", "yellow"),
        ("border", "blue"),
        ("creeper", "lime"),
        ("skull", "black"),
    ];
    for i in 0..4 {
        let layers = patterns
            .iter()
            .take(i + 2)
            .map(|(p, c)| BannerLayer {
                pattern: (*p).to_owned(),
                color: dye(c),
            })
            .collect();
        let rot = (i * 4).to_string();
        h.scene.set(
            [i as i32 * 2, 0, 0],
            state(
                ["white_banner", "red_banner", "blue_banner", "green_banner"][i],
                &[("rotation", &rot)],
            ),
            K::Banner,
            BlockEntityData::Banner(BannerData { layers }),
        );
    }
    h.scene.set(
        [8, 1, 0],
        state("light_blue_wall_banner", &[("facing", "south")]),
        K::Banner,
        BlockEntityData::Banner(BannerData {
            layers: vec![BannerLayer {
                pattern: "flower".into(),
                color: dye("pink"),
            }],
        }),
    );
    let heads = [
        "skeleton_skull",
        "wither_skeleton_skull",
        "zombie_head",
        "creeper_head",
        "dragon_head",
        "piglin_head",
        "player_head",
    ];
    for (i, n) in heads.iter().enumerate() {
        let data = if *n == "player_head" {
            BlockEntityData::Skull(SkullData {
                profile: Some(SkullProfile {
                    name: Some("Notch".into()),
                    ..SkullProfile::default()
                }),
            })
        } else {
            BlockEntityData::Skull(SkullData::default())
        };
        h.scene.set(
            [i as i32, 0, 4],
            state(n, &[("rotation", "8"), ("powered", "true")]),
            K::Skull,
            data,
        );
    }
    h.scene.set(
        [8, 0, 4],
        state("zombie_wall_head", &[("facing", "south")]),
        K::Skull,
        BlockEntityData::Skull(SkullData::default()),
    );
    row(
        &mut h,
        "banners_heads",
        DVec3::new(4.0, 3.0, 12.0),
        Vec3::new(4.0, 0.8, 2.0),
        45.0,
        12,
    );
}

#[test]
fn signs() {
    let Some(mut h) = Harness::new() else { return };
    let side = |lines: [&str; 4], color: &str, glow: bool| SignSide {
        lines: lines.map(Component::text),
        color: dye(color),
        glowing: glow,
    };
    let mut decor = Vec::new();
    let cases = [
        (
            side(["Hello", "World", "", "line 4"], "black", false),
            side(["BACK", "", "", ""], "white", false),
        ),
        (
            side(["Glowing", "orange", "text", ""], "orange", true),
            side(["", "", "", ""], "black", false),
        ),
        (
            side(["Glow black", "outline", "", ""], "black", true),
            side(["", "", "", ""], "black", false),
        ),
        (
            side(["Red", "text", "", ""], "red", false),
            side(["", "Back side", "red", ""], "cyan", true),
        ),
    ];
    for (i, (front, back)) in cases.iter().enumerate() {
        let st = state("oak_sign", &[("rotation", "8")]);
        let pos = [i as i32 * 2, 0, 0];
        decor.push((pos, st));
        h.scene.set(
            pos,
            st,
            K::Sign,
            BlockEntityData::Sign(Box::new(SignData {
                front: front.clone(),
                back: back.clone(),
                waxed: false,
            })),
        );
    }
    let wall = state("spruce_wall_sign", &[("facing", "south")]);
    decor.push(([8, 1, 0], wall));
    h.scene.set(
        [8, 1, 0],
        wall,
        K::Sign,
        BlockEntityData::Sign(Box::new(SignData {
            front: side(
                ["Wall sign", "§aGreen §lbold", "text", "end"],
                "black",
                false,
            ),
            ..SignData::default()
        })),
    );
    let hang = state(
        "oak_hanging_sign",
        &[("rotation", "8"), ("attached", "false")],
    );
    decor.push(([10, 2, 0], hang));
    h.scene.set(
        [10, 2, 0],
        hang,
        K::HangingSign,
        BlockEntityData::Sign(Box::new(SignData {
            front: side(["Hanging", "sign", "", ""], "black", false),
            back: side(["Back", "", "", ""], "blue", false),
            waxed: true,
        })),
    );
    h.decor_blocks(&decor);
    row(
        &mut h,
        "signs",
        DVec3::new(5.0, 3.0, 11.0),
        Vec3::new(5.0, 0.9, 0.0),
        40.0,
        2,
    );
    let mut back = decor.clone();
    back.truncate(decor.len());
    row(
        &mut h,
        "signs_back",
        DVec3::new(5.0, 3.0, -11.0),
        Vec3::new(5.0, 0.9, 0.0),
        40.0,
        2,
    );
}

#[test]
fn furniture() {
    let Some(mut h) = Harness::new() else { return };
    let s = &mut h.scene;
    s.set(
        [0, 0, 0],
        state("bell", &[("facing", "north"), ("attachment", "floor")]),
        K::Bell,
        BlockEntityData::None,
    );
    s.set(
        [2, 0, 0],
        state("lectern", &[("facing", "south"), ("has_book", "true")]),
        K::Lectern,
        BlockEntityData::None,
    );
    s.set(
        [4, 0, 0],
        state("enchanting_table", &[]),
        K::EnchantingTable,
        BlockEntityData::None,
    );
    let mut camp = CampfireData::default();
    camp.items[0] = Some(ItemRef {
        id: "minecraft:beef".into(),
        count: 1,
    });
    camp.items[2] = Some(ItemRef {
        id: "minecraft:cod".into(),
        count: 1,
    });
    s.set(
        [6, 0, 0],
        state("campfire", &[("facing", "south"), ("lit", "true")]),
        K::Campfire,
        BlockEntityData::Campfire(camp),
    );
    s.set(
        [8, 0, 0],
        state("decorated_pot", &[("facing", "south")]),
        K::DecoratedPot,
        BlockEntityData::Pot(PotData {
            front: Some("minecraft:angler_pottery_sherd".into()),
            back: Some("minecraft:skull_pottery_sherd".into()),
            left: Some("minecraft:brick".into()),
            right: Some("minecraft:heart_pottery_sherd".into()),
        }),
    );
    s.set(
        [10, 0, 0],
        state("conduit", &[]),
        K::Conduit,
        BlockEntityData::Conduit(ConduitData {
            active: false,
            hunting: false,
            target: None,
        }),
    );
    s.set(
        [12, 0, 0],
        state("conduit", &[]),
        K::Conduit,
        BlockEntityData::Conduit(ConduitData {
            active: true,
            hunting: true,
            target: None,
        }),
    );
    s.set(
        [14, 0, 0],
        state("copper_golem_statue", &[("facing", "south")]),
        K::CopperGolemStatue,
        BlockEntityData::None,
    );
    s.block_event([0, 0, 0], 1, 5, 0);
    h.decor_blocks(&[
        (
            [0, 0, 0],
            state("bell", &[("facing", "north"), ("attachment", "floor")]),
        ),
        (
            [2, 0, 0],
            state("lectern", &[("facing", "south"), ("has_book", "false")]),
        ),
        ([4, 0, 0], state("enchanting_table", &[])),
        (
            [6, 0, 0],
            state("campfire", &[("facing", "south"), ("lit", "true")]),
        ),
    ]);
    let frame = h.frame(
        DVec3::new(7.0, 3.5, 13.0),
        Vec3::new(7.0, 0.6, 0.5),
        50.0,
        W,
        H,
    );
    h.advance(3, &frame);
    row(
        &mut h,
        "furniture",
        DVec3::new(7.0, 3.5, 13.0),
        Vec3::new(7.0, 0.6, 0.5),
        50.0,
        3,
    );
}

#[test]
fn effects() {
    let Some(mut h) = Harness::new() else { return };
    let s = &mut h.scene;
    s.set(
        [0, 0, 0],
        state("beacon", &[]),
        K::Beacon,
        BlockEntityData::Beacon(BeaconData {
            sections: vec![
                BeamSection {
                    color: 0xF9FFFE,
                    height: 3,
                },
                BeamSection {
                    color: 0xB02E26,
                    height: 4,
                },
                BeamSection {
                    color: 0x3C44AA,
                    height: 5,
                },
            ],
        }),
    );
    s.set(
        [4, 0, 0],
        state("end_portal", &[]),
        K::EndPortal,
        BlockEntityData::None,
    );
    s.set(
        [5, 0, 0],
        state("end_portal", &[]),
        K::EndPortal,
        BlockEntityData::None,
    );
    s.set(
        [7, 0, 0],
        state("end_gateway", &[]),
        K::EndGateway,
        BlockEntityData::Gateway(GatewayData { age: 100 }),
    );
    s.set(
        [10, 0, 0],
        state("spawner", &[]),
        K::Spawner,
        BlockEntityData::Spawner(SpawnerData {
            entity_id: Some("minecraft:pig".into()),
            delay: 20,
            ..Default::default()
        }),
    );
    s.set(
        [12, 0, 0],
        state("vault", &[]),
        K::Vault,
        BlockEntityData::Vault(VaultData {
            display_item: Some(ItemRef {
                id: "minecraft:diamond".into(),
                count: 1,
            }),
            ..Default::default()
        }),
    );
    h.decor_blocks(&[
        ([0, 0, 0], state("beacon", &[])),
        ([10, 0, 0], state("spawner", &[])),
        ([12, 0, 0], state("vault", &[])),
    ]);
    row(
        &mut h,
        "effects",
        DVec3::new(6.0, 4.0, 14.0),
        Vec3::new(6.0, 2.5, 0.5),
        50.0,
        12,
    );
}
