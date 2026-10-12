mod common;

use common::*;
use mcr_block_entity::mcr_entity_model::ImageRgba;

#[test]
fn head_skins_are_freed_on_replace_and_bounded_under_churn() {
    let Some(mut h) = Harness::new() else { return };
    let s = &mut h.scene;
    let skin = ImageRgba::solid(64, 64, [200, 100, 50, 255]);
    let before = s.live_texture_layers(0);
    for _ in 0..50 {
        s.set_skin("https://textures/same", &skin);
    }
    assert_eq!(s.skin_count(), 1);
    assert_eq!(s.live_texture_layers(0), before + 1);
    for n in 0..2000 {
        s.set_skin(&format!("https://textures/{n}"), &skin);
    }
    assert!(s.skin_count() <= 128, "{}", s.skin_count());
    assert!(s.live_texture_layers(0) <= before + 128);
}
