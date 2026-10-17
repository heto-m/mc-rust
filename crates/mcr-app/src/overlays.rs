//! Full-screen effects drawn over the HUD: the vignette and border warning, the spyglass scope and
//! the totem pop animation.

use mcr_audio::SoundSource;
use mcr_client::ClientState;
use mcr_protocol::types::ItemStack;
use mcr_protocol::types::game::Hand;
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
    stack: ItemStack,
}

#[derive(Debug, Default)]
pub struct Overlays {
    scope: Option<SpriteId>,
    scope_loaded: bool,
    scope_scale: f32,
    totem: Option<Totem>,
    seed: u32,
    vignette: Option<SpriteId>,
    vignette_loaded: bool,
    vignette_brightness: f32,
    owned: mcr_ui::OwnedSprites,
}

/// Vanilla's `Lightmap.getBrightness` for a raw light level, without the dimension's ambient light.
pub fn level_brightness(raw: u8) -> f32 {
    let v = f32::from(raw.min(15)) / 15.0;
    v / (4.0 - 3.0 * v)
}

/// The vignette tint: darkening `alpha`, plus a red share near the world border.
pub fn vignette_tint(brightness: f32, warning: f32) -> (f32, f32) {
    let brightness = brightness.clamp(0.0, 1.0);
    if warning > 0.0 {
        let warning = warning.clamp(0.0, 1.0);
        let red = brightness * (1.0 - warning);
        let green_blue = brightness + (1.0 - brightness) * warning;
        (green_blue, (green_blue - red) * 0.5)
    } else {
        (brightness, 0.0)
    }
}

fn vignette_alpha(rgba: &[u8]) -> Vec<u8> {
    rgba.as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| [255, 255, 255, p[0]])
        .collect()
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
            vignette_brightness: 1.0,
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

    pub fn start_totem(&mut self, stack: ItemStack) {
        self.totem = Some(Totem {
            ticks: TOTEM_TICKS,
            offset: [self.random(), self.random()],
            stack,
        });
    }

    /// Eases the vignette toward the darkness at the camera, 1% a tick like vanilla.
    pub fn tick_vignette(&mut self, level_brightness: f32) {
        let target = (1.0 - level_brightness).clamp(0.0, 1.0);
        self.vignette_brightness += (target - self.vignette_brightness) * 0.01;
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
        border_warning: Option<f32>,
    ) {
        if let Some(warning) = border_warning {
            self.draw_vignette(frame, assets, warning);
        }
        if scoping {
            self.draw_scope(frame, assets);
        }
        self.draw_totem(frame, icons, partial);
    }

    fn draw_vignette(&mut self, frame: &mut UiFrame<'_>, assets: &Assets, warning: f32) {
        if !self.vignette_loaded {
            self.vignette_loaded = true;
            self.vignette = assets
                .read("assets/minecraft/textures/misc/vignette.png")
                .and_then(|b| decode_scope(&b))
                .and_then(|(w, h, px)| {
                    let sprites = frame.ui().sprites_mut();
                    self.owned.add(sprites, w, h, &vignette_alpha(&px))
                });
        }
        let Some(sprite) = self.vignette else { return };
        let (alpha, red) = vignette_tint(self.vignette_brightness, warning);
        let channel = |v: f32| (v.clamp(0.0, 1.0) * 255.0) as u32;
        let argb = channel(alpha) << 24 | channel(red) << 16;
        if argb >> 24 == 0 {
            return;
        }
        let (gui_w, gui_h) = frame.gui_size();
        frame.sprite_tinted(sprite, 0, 0, gui_w, gui_h, argb);
    }

    fn draw_scope(&mut self, frame: &mut UiFrame<'_>, assets: &Assets) {
        if !self.scope_loaded {
            self.scope_loaded = true;
            self.scope = assets
                .read("assets/minecraft/textures/misc/spyglass_scope.png")
                .and_then(|b| decode_scope(&b))
                .and_then(|(w, h, px)| self.owned.add(frame.ui().sprites_mut(), w, h, &px));
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
        let Some(view) = StackView::new(&t.stack) else {
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

/// Vanilla's `findTotem`: the first hand holding a death-protection item, else a plain totem.
pub fn totem_stack(state: &ClientState) -> ItemStack {
    use mcr_protocol::types::component::DataComponent;
    let totem = mcr_data::v26_3()
        .item_by_name("minecraft:totem_of_undying")
        .map_or(0, |i| i as i32);
    [Hand::Main, Hand::Off]
        .into_iter()
        .map(|hand| state.hand_stack(hand))
        .find(|s| {
            !s.is_empty()
                && (s.item == totem
                    || s.components
                        .added
                        .iter()
                        .any(|c| matches!(c, DataComponent::DeathProtection(_))))
        })
        .unwrap_or_else(|| ItemStack::new(totem, 1))
}

/// The mixer category vanilla's `Entity.getSoundSource` picks for an entity's own sounds.
pub fn entity_sound_source(state: &ClientState, entity_id: i32) -> SoundSource {
    if entity_id == state.player.entity_id {
        return SoundSource::Players;
    }
    let data = mcr_data::v26_3();
    let Some(kind) = state.entities.get(entity_id).map(|e| e.kind as u32) else {
        return SoundSource::Neutral;
    };
    if data.entity_type_name(kind) == Some("minecraft:player") {
        SoundSource::Players
    } else if data
        .entity_type_info(kind)
        .is_some_and(|i| i.category == "monster")
    {
        SoundSource::Hostile
    } else {
        SoundSource::Neutral
    }
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
        o.start_totem(ItemStack::default());
        for _ in 0..39 {
            o.tick(false);
        }
        assert!(o.totem.is_some());
        o.tick(false);
        assert!(o.totem.is_none());
    }

    #[test]
    fn border_warning_tints_red_and_keeps_full_strength_at_the_edge() {
        assert_eq!(vignette_tint(0.3, 0.0), (0.3, 0.0));
        let (alpha, red) = vignette_tint(0.0, 1.0);
        assert_eq!(alpha, 1.0);
        assert!(red > 0.4);
        assert!(level_brightness(15) > 0.99 && level_brightness(0) == 0.0);
    }

    #[test]
    fn vignette_eases_toward_darkness() {
        let mut o = Overlays::new();
        o.tick_vignette(1.0);
        assert!(o.vignette_brightness < 1.0 && o.vignette_brightness > 0.98);
    }

    #[test]
    fn hostile_scope_textures_are_rejected() {
        assert!(decode_scope(&[1, 2, 3]).is_none());
    }
}
