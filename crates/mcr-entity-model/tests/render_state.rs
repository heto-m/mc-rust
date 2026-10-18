mod support;

use mcr_entity_model::glam::{DVec3, Vec3};
use mcr_entity_model::mcr_protocol::types::ItemStack;
use mcr_entity_model::mcr_protocol::types::entity_data::EntityDataValue as V;
use mcr_entity_model::{BodyVisibility, EntityView};
use support::*;

const SIZE: u32 = 256;
const SKY: [u8; 3] = [120, 166, 255];

fn side_camera() -> Camera {
    Camera {
        position: DVec3::new(0.0, 11.0, 4.0),
        target: Vec3::new(0.0, 11.0, 0.0),
        fov_deg: 50.0,
    }
}

fn shoot(
    h: &mut Harness,
    cam: &Camera,
    scene_camera: [f64; 3],
    views: Vec<EntityView<'_>>,
) -> Shot {
    h.scene.set_camera(scene_camera, 180.0, 0.0);
    h.scene
        .update(&TestRegistries::vanilla(), views.into_iter());
    h.render(&cam.frame(SIZE, SIZE), SIZE, SIZE)
}

fn player<'a>(
    data: &'a Data,
    equipment: &'a [ItemStack; 8],
    body: BodyVisibility,
) -> EntityView<'a> {
    let mut v = EntityView::named("minecraft:player", &data.0);
    v.pos = [0.0, 10.0, 0.0];
    v.age_ticks = 20.0;
    v.equipment = Some(equipment);
    v.invisible = body != BodyVisibility::Visible;
    v.body = body;
    v
}

/// Covered pixel count and summed per-channel distance from the clear colour.
fn coverage(shot: &Shot) -> (usize, u64) {
    let mut covered = 0;
    let mut weight = 0u64;
    for p in shot.pixels.as_chunks::<4>().0 {
        let d: u64 = (0..3).map(|c| u64::from(p[c].abs_diff(SKY[c]))).sum();
        if d > 6 {
            covered += 1;
            weight += d;
        }
    }
    (covered, weight)
}

fn equipment(items: &[(usize, &str)]) -> [ItemStack; 8] {
    let mut eq: [ItemStack; 8] = Default::default();
    for &(slot, name) in items {
        eq[slot] = item(name);
    }
    eq
}

#[test]
fn invisible_player_draws_only_equipment_and_held_items() {
    let Some(mut h) = Harness::new() else { return };
    h.scene.set_shadows(false);
    let data = Data::default();
    let cam = side_camera();
    let bare = equipment(&[]);
    let geared = equipment(&[(0, "minecraft:diamond_sword"), (4, "minecraft:iron_helmet")]);

    let visible = coverage(&shoot(
        &mut h,
        &cam,
        [0.0, 11.0, 4.0],
        vec![player(&data, &geared, BodyVisibility::Visible)],
    ));
    let empty = coverage(&shoot(
        &mut h,
        &cam,
        [0.0, 11.0, 4.0],
        vec![player(&data, &bare, BodyVisibility::Hidden)],
    ));
    let items = coverage(&shoot(
        &mut h,
        &cam,
        [0.0, 11.0, 4.0],
        vec![player(&data, &geared, BodyVisibility::Hidden)],
    ));
    println!("visible {visible:?} empty {empty:?} items {items:?}");
    assert_eq!(empty.0, 0, "an invisible player without gear draws nothing");
    assert!(items.0 > 200, "helmet and sword stay visible");
    assert!(items.0 * 2 < visible.0, "the body is hidden");
}

#[test]
fn spectators_see_invisible_players_translucent() {
    let Some(mut h) = Harness::new() else { return };
    h.scene.set_shadows(false);
    let data = Data::default();
    let cam = side_camera();
    let bare = equipment(&[]);
    let visible = coverage(&shoot(
        &mut h,
        &cam,
        [0.0, 11.0, 4.0],
        vec![player(&data, &bare, BodyVisibility::Visible)],
    ));
    let ghost = coverage(&shoot(
        &mut h,
        &cam,
        [0.0, 11.0, 4.0],
        vec![player(&data, &bare, BodyVisibility::Translucent)],
    ));
    println!("visible {visible:?} ghost {ghost:?}");
    assert!(
        ghost.0 * 10 > visible.0 * 7,
        "the whole body is still drawn"
    );
    let mean = |c: (usize, u64)| c.1 as f32 / c.0.max(1) as f32;
    assert!(
        mean(ghost) < mean(visible) * 0.4,
        "at 15% alpha the sky shows through"
    );
}

#[test]
fn remote_player_bow_pulls_while_using() {
    let Some(mut h) = Harness::new() else { return };
    h.scene.set_shadows(false);
    let cam = Camera {
        position: DVec3::new(-2.5, 11.2, 1.5),
        target: Vec3::new(-0.3, 11.0, 0.0),
        fov_deg: 40.0,
    };
    let bow = equipment(&[(0, "minecraft:bow")]);
    let using = Data::default().with(8, V::Byte(1));
    let idle = Data::default();
    let mut render = |data: &Data, ticks: f32| {
        let mut v = player(data, &bow, BodyVisibility::Visible);
        v.use_ticks = ticks;
        shoot(&mut h, &cam, [-2.5, 11.2, 1.5], vec![v]).pixels
    };
    let drawn = render(&using, 1.0);
    let pulled = render(&using, 20.0);
    let idle_a = render(&idle, 1.0);
    let idle_b = render(&idle, 20.0);
    assert_eq!(
        idle_a, idle_b,
        "use ticks only matter while the living flag says so"
    );
    assert_ne!(drawn, pulled, "the bow model follows the pull");
}

#[test]
fn shadows_fade_with_camera_distance() {
    let Some(mut h) = Harness::new() else { return };
    h.scene.set_shadows(true);
    h.scene
        .set_shadow_ground(Some(Box::new(|_, y, _| (y == 10).then_some(1.0))));
    let data = Data::default();
    let cam = Camera {
        position: DVec3::new(0.0, 13.0, 2.5),
        target: Vec3::new(0.0, 10.0, 0.0),
        fov_deg: 60.0,
    };
    let pig = || {
        let mut v = EntityView::named("minecraft:pig", &data.0);
        v.pos = [0.0, 10.0, 0.0];
        v.age_ticks = 20.0;
        v
    };
    let shadow_at =
        |h: &mut Harness, d: f64| coverage(&shoot(h, &cam, [0.0, 10.0, d], vec![pig()])).1;
    let near = shadow_at(&mut h, 3.0);
    let mid = shadow_at(&mut h, 10.0);
    let far = shadow_at(&mut h, 17.0);
    h.scene.set_shadows(false);
    let none = shadow_at(&mut h, 3.0);
    println!("near {near} mid {mid} far {far} none {none}");
    assert!(near > mid && mid > far, "shadow fades with distance");
    assert_eq!(far, none, "no shadow beyond 16 blocks");
}
