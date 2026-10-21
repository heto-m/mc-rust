mod support;

use mcr_entity_model::EntityView;
use mcr_entity_model::glam::{DVec3, Vec3};
use support::*;

const SIZE: u32 = 400;

fn look_north(h: &mut Harness, eye: [f64; 3]) -> Camera {
    h.scene.set_shadows(false);
    h.scene.set_camera(eye, 180.0, 0.0);
    Camera {
        position: DVec3::from_array(eye),
        target: Vec3::new(eye[0] as f32, eye[1] as f32, eye[2] as f32 - 1.0),
        fov_deg: 70.0,
    }
}

#[test]
fn elder_guardian_curse_draws_in_front_of_the_camera() {
    let Some(mut h) = Harness::new() else { return };
    let eye = [0.0, 10.0, 0.0];
    let cam = look_north(&mut h, eye);
    let data = Data::default();
    let render = |h: &mut Harness, progress: f32| {
        let mut v = EntityView::named("minecraft:elder_guardian", &data.0);
        v.pos = eye;
        v.ghost = Some(progress);
        v.light = [15, 15];
        h.scene
            .update(&TestRegistries::vanilla(), std::iter::once(v));
        let shot = h.render(&cam.frame(SIZE, SIZE), SIZE, SIZE);
        shot.save(&format!("parity5_curse_{progress}"));
        shot
    };
    let half_coverage = |shot: &Shot, top: bool| {
        let rows = if top { 0..SIZE / 2 } else { SIZE / 2..SIZE };
        let sky = [120u8, 166, 255];
        rows.flat_map(|y| (0..SIZE).map(move |x| ((y * SIZE + x) * 4) as usize))
            .filter(|&i| (0..3).any(|c| shot.pixels[i + c].abs_diff(sky[c]) > 14))
            .count()
    };
    let start = render(&mut h, 0.1);
    assert!(
        half_coverage(&start, true) > 5 * half_coverage(&start, false).max(100),
        "it starts above the view"
    );
    let middle = render(&mut h, 0.4);
    let centre = ((SIZE / 2 * SIZE + SIZE / 2) * 4) as usize;
    assert!(
        middle.covered() > 40_000,
        "it fills the view: {}",
        middle.covered()
    );
    assert!(
        (0..3).any(|c| middle.pixels[centre + c].abs_diff([120u8, 166, 255][c]) > 14),
        "it faces the camera"
    );
}

fn changed(a: &Shot, b: &Shot, rect: [u32; 4]) -> usize {
    let [x0, y0, x1, y1] = rect;
    (y0..y1)
        .flat_map(|y| (x0..x1).map(move |x| ((y * a.width + x) * 4) as usize))
        .filter(|&i| (0..3).any(|c| a.pixels[i + c].abs_diff(b.pixels[i + c]) > 10))
        .count()
}

fn redness(shot: &Shot, rect: [u32; 4]) -> usize {
    let [x0, y0, x1, y1] = rect;
    (y0..y1)
        .flat_map(|y| (x0..x1).map(move |x| ((y * shot.width + x) * 4) as usize))
        .filter(|&i| shot.pixels[i] > 150 && shot.pixels[i + 1] < 80 && shot.pixels[i + 2] < 80)
        .count()
}

fn red_cape_skin() -> mcr_entity_model::Skin {
    mcr_entity_model::Skin {
        key: "parity5".into(),
        width: 64,
        height: 64,
        rgba: [90u8, 120, 90, 255].repeat(64 * 64),
        slim: false,
        cape: Some(mcr_entity_model::Cape {
            key: "parity5".into(),
            width: 64,
            height: 32,
            rgba: [220u8, 20, 20, 255].repeat(64 * 32),
        }),
    }
}

#[test]
fn elytra_wear_the_cape() {
    let Some(mut h) = Harness::new() else { return };
    let eye = [0.0, 11.0, -3.0];
    let cam = Camera {
        position: DVec3::from_array(eye),
        target: Vec3::new(0.0, 10.9, 0.0),
        fov_deg: 50.0,
    };
    h.scene.set_shadows(false);
    h.scene.set_camera(eye, 0.0, 0.0);
    let skin = red_cape_skin();
    let mut eq: [mcr_entity_model::mcr_protocol::types::ItemStack; 8] = Default::default();
    eq[mcr_entity_model::slot::CHEST] = item("minecraft:elytra");
    let data = Data::default();
    let render = |h: &mut Harness, parts: Option<u8>| {
        let data = match parts {
            Some(mask) => Data::default().with(
                mcr_entity_model::idx::PLAYER_MODEL_PARTS,
                mcr_entity_model::mcr_protocol::types::entity_data::EntityDataValue::Byte(
                    mask as i8,
                ),
            ),
            None => Data(data.0.clone()),
        };
        let mut v = EntityView::named("minecraft:player", &data.0);
        v.pos = [0.0, 10.0, 0.0];
        v.light = [15, 15];
        v.equipment = Some(&eq);
        v.player.skin = Some(&skin);
        h.scene
            .update(&TestRegistries::vanilla(), std::iter::once(v));
        h.render(&cam.frame(SIZE, SIZE), SIZE, SIZE)
    };
    let caped = render(&mut h, None);
    caped.save("parity5_elytra_cape");
    let hidden = render(&mut h, Some(0x7e));
    hidden.save("parity5_elytra_no_cape");
    let all = [0, 0, SIZE, SIZE];
    assert!(
        redness(&caped, all) > 2_000,
        "cape on the wings: {}",
        redness(&caped, all)
    );
    assert_eq!(
        redness(&hidden, all),
        0,
        "the item texture without the cape part"
    );
}

#[test]
fn boat_water_patch_masks_what_is_drawn_after() {
    let Some(mut h) = Harness::new() else { return };
    let eye = [0.0, 11.9, 0.0];
    let cam = Camera {
        position: DVec3::from_array(eye),
        target: Vec3::new(0.0, 10.9, -0.0175),
        fov_deg: 70.0,
    };
    h.scene.set_shadows(false);
    h.scene.set_camera(eye, 180.0, 89.0);
    let data = Data::default();
    let render = |h: &mut Harness, under_water: bool, inside: bool| {
        let mut boat = EntityView::named("minecraft:oak_boat", &data.0);
        boat.pos = [0.0, 10.0, 0.0];
        boat.light = [15, 15];
        boat.vehicle.under_water = under_water;
        let mut pig = EntityView::named("minecraft:pig", &data.0);
        pig.id = 2;
        pig.pos = [0.0, 10.1, 0.0];
        pig.scale = 0.3;
        pig.light = [15, 15];
        pig.body = mcr_entity_model::BodyVisibility::Translucent;
        h.scene.update(
            &TestRegistries::vanilla(),
            std::iter::once(boat).chain(inside.then_some(pig)),
        );
        h.render(&cam.frame(SIZE, SIZE), SIZE, SIZE)
    };
    let plain = render(&mut h, false, false);
    let unmasked = render(&mut h, true, false);
    let hull = [SIZE / 2 - 20, SIZE / 2 - 20, SIZE / 2 + 20, SIZE / 2 + 20];
    assert_eq!(
        changed(&plain, &unmasked, [0, 0, SIZE, SIZE]),
        0,
        "the patch draws no colour"
    );
    let masked = render(&mut h, false, true);
    let through = render(&mut h, true, true);
    masked.save("parity5_boat_masked");
    through.save("parity5_boat_unmasked");
    assert!(
        changed(&plain, &through, hull) > 300,
        "the translucent layer shows in the hull"
    );
    assert!(
        changed(&plain, &masked, hull) < 50,
        "the patch hides it: {}",
        changed(&plain, &masked, hull)
    );
}

#[test]
fn later_map_icons_cover_names() {
    let Some(mut h) = Harness::new() else { return };
    let eye = [0.0, 10.0, 0.9];
    let cam = look_north(&mut h, eye);
    let mut stack = item("minecraft:filled_map");
    stack.components.added.push(
        mcr_entity_model::mcr_protocol::types::component::DataComponent::MapId(mcr_buf::VarInt(5)),
    );
    let data = Data::default()
        .with(
            mcr_entity_model::idx::FRAME_DIRECTION,
            mcr_entity_model::mcr_protocol::types::entity_data::EntityDataValue::Direction(
                mcr_entity_model::mcr_protocol::types::entity_data::Direction::South,
            ),
        )
        .with(
            mcr_entity_model::idx::FRAME_ITEM,
            mcr_entity_model::mcr_protocol::types::entity_data::EntityDataValue::ItemStack(stack),
        );
    let render = |h: &mut Harness, count: usize| {
        let icon = mcr_entity_model::MapDecoration {
            sprite: "player".into(),
            x: 0,
            y: 0,
            rot: 0,
            on_frame: true,
        };
        let side = 128 * mcr_entity_model::LABEL_SCALE;
        h.scene.set_map(
            5,
            mcr_entity_model::ImageRgba::solid(128, 128, [200, 200, 200, 255]),
            vec![icon; count],
            Some(mcr_entity_model::MapLabels {
                held: Some(mcr_entity_model::ImageRgba::solid(
                    side,
                    side,
                    [230, 10, 10, 255],
                )),
                frame: None,
                frame_same: true,
            }),
        );
        let mut v = EntityView::named("minecraft:item_frame", &data.0);
        v.pos = [-0.5, 9.5, -0.5];
        v.light = [15, 15];
        h.scene
            .update(&TestRegistries::vanilla(), std::iter::once(v));
        let shot = h.render(&cam.frame(SIZE, SIZE), SIZE, SIZE);
        shot.save(&format!("parity5_map_{count}"));
        shot
    };
    let few = render(&mut h, 10);
    let many = render(&mut h, 40);
    let icon = [SIZE / 2 - 6, SIZE / 2 - 6, SIZE / 2 + 6, SIZE / 2 + 6];
    assert_eq!(redness(&few, icon), 144, "names over the first icons");
    assert!(
        redness(&many, icon) < 100,
        "the 37th icon covers the names: {}",
        redness(&many, icon)
    );
}
