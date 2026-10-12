//! Text display entities: laid out once per text change, drawn as glyph geometry through the
//! block-entity flat text pipeline (vanilla's `TextDisplayRenderer`).

use std::sync::Arc;

use mcr_block_entity::{Align, DisplayLayout, TextDisplayDraw, layout_display};
use mcr_client::Entity;
use mcr_entity_model::glam::Mat4;
use mcr_entity_model::mcr_protocol::types::entity_data::EntityDataValue;
use mcr_entity_model::{DisplaySample, idx};
use mcr_ui::Ui;
use rustc_hash::FxHashMap;

const MAX_DRAWS: usize = 128;
const MAX_LAYOUTS: usize = 256;
const DEFAULT_LINE_WIDTH: i32 = 200;
const FLAG_SHADOW: i8 = 1;
const FLAG_SEE_THROUGH: i8 = 2;
const FLAG_DEFAULT_BACKGROUND: i8 = 4;
const FLAG_LEFT: i8 = 8;
const FLAG_RIGHT: i8 = 16;

#[derive(Debug)]
struct Cached {
    rev: u64,
    line_width: i32,
    align: Align,
    shadow: bool,
    layout: Arc<DisplayLayout>,
}

#[derive(Debug, Default)]
pub struct TextDisplays {
    layouts: FxHashMap<i32, Cached>,
    draws: Vec<TextDisplayDraw>,
}

fn flags(e: &Entity) -> i8 {
    match e.data.get(&idx::TEXT_DISPLAY_FLAGS) {
        Some(EntityDataValue::Byte(v)) => *v,
        _ => 0,
    }
}

fn align(flags: i8) -> Align {
    if flags & FLAG_LEFT != 0 {
        Align::Left
    } else if flags & FLAG_RIGHT != 0 {
        Align::Right
    } else {
        Align::Center
    }
}

/// `TextDisplayRenderer`: the default background is black at the options' background opacity.
fn background(flags: i8, sample: &DisplaySample, default_opacity: f32) -> u32 {
    if flags & FLAG_DEFAULT_BACKGROUND != 0 {
        ((default_opacity.clamp(0.0, 1.0) * 255.0) as u32) << 24
    } else {
        sample.background
    }
}

impl TextDisplays {
    pub fn begin(&mut self) {
        self.draws.clear();
    }

    pub fn take(&mut self) -> Vec<TextDisplayDraw> {
        std::mem::take(&mut self.draws)
    }

    pub fn forget_missing(&mut self, alive: impl Fn(i32) -> bool) {
        self.layouts.retain(|id, _| alive(*id));
    }

    #[allow(clippy::too_many_arguments)]
    pub fn push(
        &mut self,
        ui: &mut Ui,
        e: &Entity,
        sample: &DisplaySample,
        local: Mat4,
        origin: [f64; 3],
        light: [u8; 2],
        default_opacity: f32,
    ) {
        if self.draws.len() >= MAX_DRAWS || !local.is_finite() {
            return;
        }
        let flags = flags(e);
        let line_width = match e.data.get(&idx::TEXT_DISPLAY_LINE_WIDTH) {
            Some(EntityDataValue::Int(v)) => *v,
            _ => DEFAULT_LINE_WIDTH,
        };
        let (align, shadow) = (align(flags), flags & FLAG_SHADOW != 0);
        let fresh = |c: &Cached| {
            c.rev == sample.text_rev
                && c.line_width == line_width
                && c.align == align
                && c.shadow == shadow
        };
        if !self.layouts.get(&e.id).is_some_and(fresh) {
            if !self.layouts.contains_key(&e.id) && self.layouts.len() >= MAX_LAYOUTS {
                return;
            }
            let cells = match e.data.get(&idx::TEXT_DISPLAY_TEXT) {
                Some(EntityDataValue::Component(c)) => ui.flatten(c),
                _ => Vec::new(),
            };
            let layout = Arc::new(layout_display(ui, &cells, line_width, align, shadow));
            self.layouts.insert(
                e.id,
                Cached {
                    rev: sample.text_rev,
                    line_width,
                    align,
                    shadow,
                    layout,
                },
            );
        }
        let Some(cached) = self.layouts.get(&e.id) else {
            return;
        };
        self.draws.push(TextDisplayDraw {
            layout: Arc::clone(&cached.layout),
            origin,
            local,
            opacity: sample.opacity,
            background: background(flags, sample, default_opacity),
            see_through: flags & FLAG_SEE_THROUGH != 0,
            light,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(background: u32) -> DisplaySample {
        DisplaySample {
            transform: Mat4::IDENTITY,
            opacity: 255,
            background,
            text_rev: 0,
        }
    }

    #[test]
    fn alignment_flags_pick_left_before_right() {
        assert_eq!(align(0), Align::Center);
        assert_eq!(align(FLAG_LEFT), Align::Left);
        assert_eq!(align(FLAG_RIGHT), Align::Right);
        assert_eq!(align(FLAG_LEFT | FLAG_RIGHT), Align::Left);
    }

    #[test]
    fn default_background_follows_the_option_not_the_entity() {
        let s = sample(0x80FF_0000);
        assert_eq!(background(0, &s, 0.25), 0x80FF_0000);
        assert_eq!(background(FLAG_DEFAULT_BACKGROUND, &s, 0.25), 63 << 24);
        assert_eq!(background(FLAG_DEFAULT_BACKGROUND, &s, 5.0), 255 << 24);
    }
}
