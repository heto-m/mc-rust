mod support;

use mcr_entity_model::glam::{DVec3, Vec3};
use mcr_entity_model::{EntityView, LightInfo};
use support::*;

const SIZE: u32 = 160;

fn render_lit(h: &mut Harness, light: [u8; 2], entry: f32) -> Shot {
    let data = Data::default();
    let mut view = EntityView::named("minecraft:zombie", &data.0);
    view.light = light;
    h.scene
        .update(&TestRegistries::vanilla(), std::iter::once(view));
    let mut lightmap = [[0.0, 0.0, 0.0, 1.0]; 256];
    lightmap[usize::from(light[1]) * 16 + usize::from(light[0])] = [entry, entry, entry, 1.0];
    let mut frame = Camera {
        position: DVec3::new(0.0, 1.2, 3.5),
        target: Vec3::new(0.0, 1.0, 0.0),
        fov_deg: 40.0,
    }
    .frame(SIZE, SIZE);
    frame.light = LightInfo::with_lightmap(&lightmap);
    h.render(&frame, SIZE, SIZE)
}

/// Mean brightness over the pixels that differ from the clear colour.
fn mean_covered(shot: &Shot) -> f32 {
    let sky = [120u8, 166, 255];
    let covered: Vec<f32> = shot
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| (0..3).any(|i| p[i].abs_diff(sky[i]) > 14))
        .map(|p| f32::from(p[0]) + f32::from(p[1]) + f32::from(p[2]))
        .collect();
    assert!(covered.len() > 200, "entity not drawn");
    covered.iter().sum::<f32>() / covered.len() as f32
}

#[test]
fn entities_take_their_light_from_the_world_lightmap_entry() {
    let Some(mut h) = Harness::new() else { return };
    let light = [3, 8];
    let full = mean_covered(&render_lit(&mut h, light, 1.0));
    let half = mean_covered(&render_lit(&mut h, light, 0.5));
    let dark = mean_covered(&render_lit(&mut h, light, 0.0));
    assert!(dark < 1.0, "unlit entry still lights the entity: {dark}");
    assert!(
        (half / full - 0.5).abs() < 0.05,
        "entity colour must scale with its lightmap entry: {half} vs {full}"
    );
}

#[test]
fn light_info_colour_reads_the_sky_major_entry() {
    let mut lightmap = [[0.0; 4]; 256];
    lightmap[8 * 16 + 3] = [0.25, 0.5, 0.75, 1.0];
    assert_eq!(
        LightInfo::with_lightmap(&lightmap).color([3, 8]),
        [0.25, 0.5, 0.75]
    );
}
