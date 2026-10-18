mod common;

use mcr_assets::{SpriteId, TextureArrayBuilder, decode_sprite};

/// Share of texels whose alpha passes the cutout test.
fn passing(rgba: &[u8]) -> f32 {
    let px = rgba.as_chunks::<4>().0;
    px.iter().filter(|p| p[3] > 127).count() as f32 / px.len() as f32
}

#[test]
fn kelp_frames_stay_visible_at_every_mip() {
    if !common::jar_path().exists() {
        eprintln!("skipping: no client jar");
        return;
    }
    let resources = common::vanilla();
    let path = "assets/minecraft/textures/block/kelp_plant.png";
    let png = resources.read(path).unwrap().expect("kelp_plant.png");
    let meta = resources.read(&format!("{path}.mcmeta")).unwrap();
    let sprite = decode_sprite(path, &png, meta.as_deref()).unwrap();
    let mut b = TextureArrayBuilder::new(4);
    let slot = b.add(SpriteId(0), &sprite);
    let arrays = b.build();
    let group = &arrays.groups[slot.group as usize];
    for level in 3..=4 {
        for frame in 0..slot.layer_count {
            let c = passing(group.layer_bytes(level, slot.first_layer + frame));
            assert!(c > 0.0, "frame {frame} vanishes at mip {level}");
        }
    }
}
