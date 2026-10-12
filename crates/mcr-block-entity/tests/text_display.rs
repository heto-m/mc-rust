mod common;

use std::sync::Arc;

use common::*;
use mcr_block_entity::mcr_entity_model::glam::{DVec3, Mat4, Vec3};
use mcr_block_entity::mcr_entity_model::mcr_protocol::types::Component;
use mcr_block_entity::{Align, DisplayLayout, TextDisplayDraw, layout_display};

const TEXT: &str = "Hello display\nSecond line that is long enough to wrap around twice";

fn layout(h: &mut Harness, width: i32, align: Align) -> DisplayLayout {
    let cells = h.ui.flatten(&Component::text(TEXT));
    layout_display(&mut h.ui, &cells, width, align, true)
}

fn draw(layout: DisplayLayout, see_through: bool) -> TextDisplayDraw {
    TextDisplayDraw {
        layout: Arc::new(layout),
        origin: [0.0, 1.0, 0.0],
        local: Mat4::IDENTITY,
        opacity: 255,
        background: 0x4000_0000,
        see_through,
        light: [15, 15],
    }
}

fn first_x(layout: &DisplayLayout) -> f32 {
    layout
        .quads
        .iter()
        .map(|q| q.pos[0][0])
        .fold(f32::MAX, f32::min)
}

#[test]
fn lines_wrap_to_the_requested_width_and_align() {
    let Some(mut h) = Harness::new() else { return };
    let narrow = layout(&mut h, 60, Align::Center);
    assert!(narrow.width <= 60 && narrow.width > 0, "{}", narrow.width);
    assert!(narrow.height >= 3 * 10 - 1, "{}", narrow.height);
    assert_eq!((narrow.height + 1) % 10, 0);
    let left = layout(&mut h, 60, Align::Left);
    let right = layout(&mut h, 60, Align::Right);
    assert_eq!(left.height, right.height);
    assert!(first_x(&left) <= first_x(&right));
}

#[test]
fn hostile_text_and_widths_stay_bounded() {
    let Some(mut h) = Harness::new() else { return };
    let cells = h.ui.flatten(&Component::text("x\n".repeat(50_000)));
    let l = layout_display(&mut h.ui, &cells, i32::MAX, Align::Center, true);
    assert!(l.height <= 64 * 10);
    let l = layout_display(&mut h.ui, &cells, i32::MIN, Align::Left, false);
    assert!(l.width <= 1024);
}

#[test]
fn text_draws_and_see_through_ignores_a_wall() {
    let Some(mut h) = Harness::new() else { return };
    let wall: Vec<([i32; 3], u32)> = (-3..3)
        .flat_map(|x| (0..3).map(move |y| ([x, y, 2], state("stone", &[]))))
        .collect();
    h.decor_blocks(&wall);
    let frame = h.frame(
        DVec3::new(0.0, 1.5, 7.0),
        Vec3::new(0.0, 1.5, 0.0),
        60.0,
        640,
        360,
    );
    let l = layout(&mut h, 120, Align::Center);
    h.scene.queue_text(vec![draw(l.clone(), false)]);
    let hidden = h.render(&frame, 640, 360);
    hidden.save("text_display_hidden");
    h.scene.queue_text(vec![draw(l, true)]);
    let shown = h.render(&frame, 640, 360);
    shown.save("text_display_see_through");
    let differing = hidden
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .zip(shown.pixels.as_chunks::<4>().0)
        .filter(|(a, b)| a != b)
        .count();
    assert!(differing > 300, "{differing}");
}

#[test]
fn text_is_drawn_without_a_wall() {
    let Some(mut h) = Harness::new() else { return };
    let frame = h.frame(
        DVec3::new(0.0, 1.5, 4.0),
        Vec3::new(0.0, 1.5, 0.0),
        60.0,
        640,
        360,
    );
    h.render(&frame, 640, 360);
    let l = layout(&mut h, 120, Align::Center);
    h.scene.queue_text(vec![draw(l, false)]);
    let shot = h.render(&frame, 640, 360);
    shot.save("text_display");
    assert!(shot.covered() > 500, "{}", shot.covered());
}
