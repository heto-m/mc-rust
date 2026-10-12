mod support;

use mcr_entity_model::EntityView;
use mcr_entity_model::glam::{DVec3, Vec3};
use support::*;

const SIZE: u32 = 600;
const DISTANCE: f64 = 6.0;
const FOV: f32 = 30.0;

/// World heights, relative to the entity origin, of the lowest and highest covered pixel rows.
fn vertical_extent(h: &mut Harness, ty: &str, yaw: f32, pitch: f32) -> Option<(f32, f32)> {
    let data = Data::default();
    let mut v = EntityView::named(ty, &data.0);
    v.pos = [0.0, 10.0, 0.0];
    v.yaw = yaw;
    v.body_yaw = yaw;
    v.pitch = pitch;
    v.age_ticks = 40.0;
    h.scene.set_shadows(false);
    h.scene.set_camera([0.0, 10.0, DISTANCE], 180.0, 0.0);
    h.scene
        .update(&TestRegistries::vanilla(), std::iter::once(v));
    let cam = Camera {
        position: DVec3::new(0.0, 10.0, DISTANCE),
        target: Vec3::new(0.0, 10.0, 0.0),
        fov_deg: FOV,
    };
    let shot = h.render(&cam.frame(SIZE, SIZE), SIZE, SIZE);
    let sky = [120u8, 166, 255];
    let rows: Vec<u32> = (0..SIZE)
        .filter(|row| {
            let start = (row * SIZE * 4) as usize;
            shot.pixels[start..start + (SIZE * 4) as usize]
                .as_chunks::<4>()
                .0
                .iter()
                .any(|p| (0..3).any(|c| p[c].abs_diff(sky[c]) > 14))
        })
        .collect();
    let (&top, &bottom) = (rows.first()?, rows.last()?);
    let half = DISTANCE as f32 * (FOV.to_radians() / 2.0).tan();
    let y = |row: u32| (SIZE as f32 / 2.0 - row as f32) / (SIZE as f32 / 2.0) * half;
    Some((y(bottom + 1), y(top)))
}

/// Vanilla heights of each non-living model: the model's own offsets after its renderer's pose stack.
#[test]
fn non_living_models_sit_where_vanilla_puts_them() {
    let Some(mut h) = Harness::new() else { return };
    let cases: &[(&str, f32, f32, f32, f32)] = &[
        ("minecraft:leash_knot", 0.0, 0.0, 0.0, 0.5),
        ("minecraft:llama_spit", 0.0, 0.0, -0.1, 0.4),
        ("minecraft:shulker_bullet", 0.0, 0.0, -0.1, 0.45),
        ("minecraft:wind_charge", 0.0, 0.0, -0.3, 0.3),
        ("minecraft:breeze_wind_charge", 0.0, 0.0, -0.3, 0.3),
        ("minecraft:trident", 0.0, 0.0, -0.5, 0.5),
        ("minecraft:wither_skull", 0.0, 0.0, 0.0, 0.5),
        ("minecraft:end_crystal", 0.0, 0.0, -1.2, 1.7),
        ("minecraft:minecart", 0.0, 0.0, 0.0, 0.8),
        ("minecraft:oak_boat", 0.0, 0.0, 0.0, 0.9),
        ("minecraft:arrow", 0.0, 0.0, -0.2, 0.2),
    ];
    for &(ty, yaw, pitch, low, high) in cases {
        let Some((bottom, top)) = vertical_extent(&mut h, ty, yaw, pitch) else {
            panic!("{ty} drew nothing");
        };
        println!("{ty}: {bottom:.2}..{top:.2}");
        let slack = 0.12;
        assert!(
            bottom >= low - slack && top <= high + slack,
            "{ty} spans {bottom:.2}..{top:.2}, expected within {low}..{high}"
        );
    }
}

fn render_with_leash(h: &mut Harness, leash: Option<([f64; 3], [f64; 3])>) -> Vec<u8> {
    let data = Data::default();
    let mut v = EntityView::named("minecraft:leash_knot", &data.0);
    v.pos = [0.0, 10.0, 0.0];
    v.age_ticks = 40.0;
    v.leash = leash;
    h.scene.set_shadows(false);
    h.scene.set_camera([0.0, 10.0, DISTANCE], 180.0, 0.0);
    h.scene
        .update(&TestRegistries::vanilla(), std::iter::once(v));
    let cam = Camera {
        position: DVec3::new(0.0, 10.0, DISTANCE),
        target: Vec3::new(0.0, 10.0, 0.0),
        fov_deg: FOV,
    };
    h.render(&cam.frame(SIZE, SIZE), SIZE, SIZE).pixels
}

#[test]
fn leash_rope_is_drawn_between_its_ends() {
    let Some(mut h) = Harness::new() else { return };
    let bare = render_with_leash(&mut h, None);
    let roped = render_with_leash(&mut h, Some(([0.0, 10.3, 0.0], [1.5, 10.3, 0.0])));
    let changed = bare
        .as_chunks::<4>()
        .0
        .iter()
        .zip(roped.as_chunks::<4>().0)
        .filter(|(a, b)| a != b)
        .count();
    assert!(changed > 100, "only {changed} pixels changed");
    let far = render_with_leash(&mut h, Some(([0.0, 10.3, 0.0], [5000.0, 10.3, 0.0])));
    assert_eq!(far, bare, "absurdly long ropes are dropped");
}
