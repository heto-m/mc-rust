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

fn blue_elytra_skin() -> mcr_entity_model::Skin {
    mcr_entity_model::Skin {
        elytra: Some(mcr_entity_model::Cape {
            key: "parity6-elytra".into(),
            width: 64,
            height: 32,
            rgba: [20u8, 20, 220, 255].repeat(64 * 32),
        }),
        ..red_cape_skin()
    }
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
        elytra: None,
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
fn profile_elytra_texture_beats_the_cape() {
    let Some(mut h) = Harness::new() else { return };
    let eye = [0.0, 11.0, -3.0];
    let cam = Camera {
        position: DVec3::from_array(eye),
        target: Vec3::new(0.0, 10.9, 0.0),
        fov_deg: 50.0,
    };
    h.scene.set_shadows(false);
    h.scene.set_camera(eye, 0.0, 0.0);
    let skin = blue_elytra_skin();
    let mut eq: [mcr_entity_model::mcr_protocol::types::ItemStack; 8] = Default::default();
    eq[mcr_entity_model::slot::CHEST] = item("minecraft:elytra");
    let data = Data::default().with(
        mcr_entity_model::idx::PLAYER_MODEL_PARTS,
        mcr_entity_model::mcr_protocol::types::entity_data::EntityDataValue::Byte(0x7e),
    );
    let mut v = EntityView::named("minecraft:player", &data.0);
    v.pos = [0.0, 10.0, 0.0];
    v.light = [15, 15];
    v.equipment = Some(&eq);
    v.player.skin = Some(&skin);
    h.scene
        .update(&TestRegistries::vanilla(), std::iter::once(v));
    let shot = h.render(&cam.frame(SIZE, SIZE), SIZE, SIZE);
    shot.save("parity6_profile_elytra");
    let all = [0, 0, SIZE, SIZE];
    let blue = (0..SIZE * SIZE)
        .map(|i| (i * 4) as usize)
        .filter(|&i| shot.pixels[i + 2] > 150 && shot.pixels[i] < 80 && shot.pixels[i + 1] < 80)
        .count();
    assert!(blue > 2_000, "profile elytra on the wings: {blue}");
    assert_eq!(redness(&shot, all), 0, "the cape stays off the wings");
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

fn diff_centroid(a: &Shot, b: &Shot) -> Option<(f32, f32)> {
    let (mut n, mut sx, mut sy) = (0u32, 0.0f32, 0.0f32);
    for y in 0..a.height {
        for x in 0..a.width {
            let i = ((y * a.width + x) * 4) as usize;
            if (0..3).any(|c| a.pixels[i + c].abs_diff(b.pixels[i + c]) > 10) {
                n += 1;
                sx += x as f32;
                sy += y as f32;
            }
        }
    }
    (n > 20).then(|| (sx / n as f32, sy / n as f32))
}

#[test]
fn map_markers_and_names_share_the_pictures_orientation() {
    let Some(mut h) = Harness::new() else { return };
    let eye = [0.0, 10.0, 0.9];
    let cam = look_north(&mut h, eye);
    let mut stack = item("minecraft:filled_map");
    stack.components.added.push(
        mcr_entity_model::mcr_protocol::types::component::DataComponent::MapId(mcr_buf::VarInt(6)),
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
    let mut image = mcr_entity_model::ImageRgba::solid(128, 128, [200, 200, 200, 255]);
    for y in 0..32usize {
        for x in 0..32usize {
            image.rgba[(y * 128 + x) * 4..][..4].copy_from_slice(&[230, 10, 10, 255]);
        }
    }
    let side = (128 * mcr_entity_model::LABEL_SCALE) as usize;
    let mut names = mcr_entity_model::ImageRgba::solid(side as u32, side as u32, [0; 4]);
    for y in 8 * 4..28 * 4 {
        for x in 100 * 4..120 * 4 {
            names.rgba[(y * side + x) * 4..][..4].copy_from_slice(&[10, 10, 230, 255]);
        }
    }
    let render = |h: &mut Harness, marker: bool, labels: bool| {
        let decorations = if marker {
            vec![mcr_entity_model::MapDecoration {
                sprite: "player".into(),
                x: 100,
                y: -100,
                rot: 0,
                on_frame: true,
            }]
        } else {
            Vec::new()
        };
        h.scene.set_map(
            6,
            image.clone(),
            decorations,
            Some(mcr_entity_model::MapLabels {
                held: labels.then(|| names.clone()),
                frame: None,
                frame_same: true,
            }),
        );
        let mut v = EntityView::named("minecraft:item_frame", &data.0);
        v.pos = [-0.5, 9.5, -0.5];
        v.light = [15, 15];
        h.scene
            .update(&TestRegistries::vanilla(), std::iter::once(v));
        h.render(&cam.frame(SIZE, SIZE), SIZE, SIZE)
    };
    let plain = render(&mut h, false, false);
    let marked = render(&mut h, true, false);
    let named = render(&mut h, false, true);
    let (mx, my) = diff_centroid(&plain, &marked).expect("the marker draws");
    let (nx, ny) = diff_centroid(&plain, &named).expect("the names draw");
    let centre = SIZE as f32 / 2.0;
    plain.save("parity6_map_plain");
    marked.save("parity6_map_marker");
    named.save("parity6_map_names");
    let red = redness(&plain, [0, 0, SIZE, SIZE]);
    assert!(red > 100);
    let (rx, ry) = {
        let (mut n, mut sx, mut sy) = (0u32, 0.0f32, 0.0f32);
        for y in 0..SIZE {
            for x in 0..SIZE {
                let i = ((y * SIZE + x) * 4) as usize;
                if plain.pixels[i] > 150 && plain.pixels[i + 1] < 80 && plain.pixels[i + 2] < 80 {
                    n += 1;
                    sx += x as f32;
                    sy += y as f32;
                }
            }
        }
        (sx / n as f32, sy / n as f32)
    };
    assert!(
        mx > centre && my < centre,
        "marker at the north east: {mx},{my}"
    );
    assert!(
        nx > centre && ny < centre,
        "names at the north east: {nx},{ny}"
    );
    assert!(
        rx < centre && ry < centre,
        "the picture's red corner is north west: {rx},{ry}"
    );
}
