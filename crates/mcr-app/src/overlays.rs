//! Full-screen effects drawn over the HUD: the spyglass scope and the totem pop animation.

use mcr_protocol::types::ItemStack;
use mcr_ui::{ItemIcons, ItemView, SpriteId, StackView, UiFrame};

use crate::assets::Assets;

const TOTEM_TICKS: i32 = 40;
const SCOPE_TARGET: f32 = 1.125;
const SCOPE_REST: f32 = 0.5;
const MAX_SCOPE_PIXELS: u32 = 1024;

#[derive(Debug)]
struct Totem {
    ticks: i32,
    offset: [f32; 2],
}

#[derive(Debug, Default)]
pub struct Overlays {
    scope: Option<SpriteId>,
    scope_loaded: bool,
    scope_scale: f32,
    totem: Option<Totem>,
    seed: u32,
}

fn decode_scope(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let info = reader.info();
    let (w, h) = (info.width, info.height);
    if w == 0 || h == 0 || w > MAX_SCOPE_PIXELS || h > MAX_SCOPE_PIXELS {
        return None;
    }
    let mut buf = vec![0u8; reader.output_buffer_size()?];
    let frame = reader.next_frame(&mut buf).ok()?;
    let data = &buf[..frame.buffer_size()];
    let rgba = match frame.color_type {
        png::ColorType::Rgba => data.to_vec(),
        png::ColorType::Rgb => data
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        _ => return None,
    };
    (rgba.len() == (w * h * 4) as usize).then_some((w, h, rgba))
}

impl Overlays {
    pub fn new() -> Self {
        Self {
            scope_scale: SCOPE_REST,
            seed: 0x9e37_79b9,
            ..Self::default()
        }
    }

    fn random(&mut self) -> f32 {
        self.seed = self
            .seed
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223);
        (self.seed >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0
    }

    pub fn start_totem(&mut self) {
        self.totem = Some(Totem {
            ticks: TOTEM_TICKS,
            offset: [self.random(), self.random()],
        });
    }

    pub fn tick(&mut self, scoping: bool) {
        if let Some(t) = &mut self.totem {
            t.ticks -= 1;
            if t.ticks <= 0 {
                self.totem = None;
            }
        }
        let target = if scoping { SCOPE_TARGET } else { SCOPE_REST };
        self.scope_scale += (target - self.scope_scale) * 0.5;
    }

    pub fn draw(
        &mut self,
        frame: &mut UiFrame<'_>,
        icons: &mut ItemIcons,
        assets: &Assets,
        scoping: bool,
        partial: f32,
    ) {
        if scoping {
            self.draw_scope(frame, assets);
        }
        self.draw_totem(frame, icons, partial);
    }

    fn draw_scope(&mut self, frame: &mut UiFrame<'_>, assets: &Assets) {
        if !self.scope_loaded {
            self.scope_loaded = true;
            self.scope = assets
                .read("assets/minecraft/textures/misc/spyglass_scope.png")
                .and_then(|b| decode_scope(&b))
                .and_then(|(w, h, px)| frame.ui().sprites_mut().add_raw(w, h, &px));
        }
        let Some(sprite) = self.scope else { return };
        let (gui_w, gui_h) = frame.gui_size();
        let size = (gui_w.min(gui_h) as f32 * self.scope_scale) as i32;
        let (x, y) = ((gui_w - size) / 2, (gui_h - size) / 2);
        frame.sprite(sprite, x, y, size, size);
        let black = 0xFF00_0000;
        frame.fill_rect(0, y + size, gui_w, gui_h, black);
        frame.fill_rect(0, 0, gui_w, y, black);
        frame.fill_rect(0, y, x, y + size, black);
        frame.fill_rect(x + size, y, gui_w, y + size, black);
    }

    fn draw_totem(&mut self, frame: &mut UiFrame<'_>, icons: &mut ItemIcons, partial: f32) {
        let Some(t) = &self.totem else { return };
        let stack = totem_stack();
        let Some(view) = StackView::new(&stack) else {
            return;
        };
        let frac = ((TOTEM_TICKS - t.ticks) as f32 + partial) / TOTEM_TICKS as f32;
        let (f2, f3) = (frac * frac, frac * frac * frac);
        let yy = 10.25 * f3 * f2 - 24.95 * f2 * f2 + 25.5 * f3 - 13.8 * f2 + 4.0 * frac;
        let angle = yy * std::f32::consts::PI;
        let (gui_w, gui_h) = frame.gui_size();
        let sway = (angle * 2.0).sin().abs();
        let cx = gui_w as f32 / 2.0 + t.offset[0] * (gui_w as f32 / 4.0) * sway;
        let cy = gui_h as f32 / 2.0 + t.offset[1] * (gui_h as f32 / 4.0) * sway;
        let size = 50.0 + 175.0 * angle.sin();
        let spin = (900.0 * angle.sin().abs())
            .to_radians()
            .cos()
            .abs()
            .max(0.12);
        let (w, h) = ((size * spin) as i32, size as i32);
        if h <= 0 {
            return;
        }
        let item = ItemView {
            item: view.name(),
            props: &view,
            glint: false,
            cooldown: 0.0,
        };
        icons.draw_sized(frame, &item, cx as i32 - w / 2, cy as i32 - h / 2, w, h);
    }
}

fn totem_stack() -> ItemStack {
    let item = mcr_data::v26_3()
        .item_by_name("minecraft:totem_of_undying")
        .map_or(0, |i| i as i32);
    ItemStack::new(item, 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_scale_converges_and_returns() {
        let mut o = Overlays::new();
        for _ in 0..12 {
            o.tick(true);
        }
        assert!(o.scope_scale > 1.1);
        for _ in 0..12 {
            o.tick(false);
        }
        assert!(o.scope_scale < 0.51);
    }

    #[test]
    fn totem_expires_after_forty_ticks() {
        let mut o = Overlays::new();
        o.start_totem();
        for _ in 0..39 {
            o.tick(false);
        }
        assert!(o.totem.is_some());
        o.tick(false);
        assert!(o.totem.is_none());
    }

    #[test]
    fn hostile_scope_textures_are_rejected() {
        assert!(decode_scope(&[1, 2, 3]).is_none());
    }
}
