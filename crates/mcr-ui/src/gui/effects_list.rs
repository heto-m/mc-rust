//! Vanilla's `EffectsInInventory`: the active effects beside the inventory screens.

use std::cmp::Ordering;

use mcr_protocol::types::text::Component;

use super::env::{ActiveEffect, RenderCx};
use super::tooltip::TooltipLines;
use crate::items::{effect_info, format_ticks};
use crate::style::Cell;
use crate::text::parse_legacy;

const MIN_WIDTH: i32 = 32;
const TEXT_X: i32 = 32;
const SPACING: i32 = 7;
const ROW: i32 = 33;
const MAX_EFFECTS: usize = 64;

fn color(effect: &ActiveEffect) -> i32 {
    effect_info(&effect.effect).map_or(0, |i| i.color)
}

/// What `MobEffectInstance#compareTo` looks at; `color` is the effect's particle colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectSortKey {
    pub ambient: bool,
    pub duration: i32,
    pub color: i32,
}

/// `MobEffectInstance#compareTo`.
pub fn effect_key_order(a: EffectSortKey, b: EffectSortKey) -> Ordering {
    const CUTOFF: i32 = 32147;
    let infinite = |e: EffectSortKey| e.duration == -1;
    if (a.duration <= CUTOFF || b.duration <= CUTOFF) && !(a.ambient && b.ambient) {
        a.ambient
            .cmp(&b.ambient)
            .then(infinite(a).cmp(&infinite(b)))
            .then(a.duration.cmp(&b.duration))
            .then(a.color.cmp(&b.color))
    } else {
        a.ambient.cmp(&b.ambient).then(a.color.cmp(&b.color))
    }
}

/// `MobEffectInstance#compareTo`.
pub fn effect_order(a: &ActiveEffect, b: &ActiveEffect) -> Ordering {
    let key = |e: &ActiveEffect| EffectSortKey {
        ambient: e.ambient,
        duration: e.duration,
        color: color(e),
    };
    effect_key_order(key(a), key(b))
}

/// `EffectsInInventory#getEffectName`.
pub fn effect_name(effect: &ActiveEffect) -> Component {
    let (namespace, path) = effect
        .effect
        .split_once(':')
        .unwrap_or(("minecraft", &effect.effect));
    let mut name = Component::translatable(
        format!("effect.{namespace}.{}", path.replace('/', ".")),
        Vec::new(),
    );
    if (1..=9).contains(&effect.amplifier) {
        name = name
            .with(Component::text(" "))
            .with(Component::translatable(
                format!("enchantment.level.{}", effect.amplifier + 1),
                Vec::new(),
            ));
    }
    name
}

fn duration_text(cx: &mut RenderCx<'_, '_>, effect: &ActiveEffect, tick_rate: f32) -> String {
    if effect.duration == -1 {
        cx.lang("effect.duration.infinite")
    } else {
        format_ticks(effect.duration, tick_rate)
    }
}

/// `ComponentRenderUtils#clipText`: cut to fit and end with an ellipsis.
fn clip(cx: &mut RenderCx<'_, '_>, cells: &[Cell], max: i32) -> Vec<Cell> {
    let dots = parse_legacy("...");
    let room = max as f32 - cx.frame.ui().width(&dots);
    let mut out = Vec::new();
    let mut width = 0.0;
    for c in cells {
        let w = cx.frame.ui().width(std::slice::from_ref(c));
        if width + w > room {
            break;
        }
        width += w;
        out.push(*c);
    }
    let style = cells.last().map(|c| c.style).unwrap_or_default();
    out.extend(dots.into_iter().map(|mut c| {
        c.style = style;
        c
    }));
    out
}

/// Whether the effects fit right of a container image ending at `right`; vanilla hides them otherwise.
pub fn can_see_effects(gui_width: i32, right: i32) -> bool {
    gui_width - (right + 2) >= MIN_WIDTH
}

/// Draws the player's effects right of the container image at `left`..`left + width`, top `top`.
pub fn draw_effects(cx: &mut RenderCx<'_, '_>, left: i32, top: i32, width: i32) {
    let mut effects = cx.view.active_effects();
    let x0 = left + width + 2;
    let available = cx.gui_size().0 - x0;
    if effects.is_empty() || available < MIN_WIDTH {
        return;
    }
    effects.truncate(MAX_EFFECTS);
    effects.sort_by(effect_order);
    let max_width = if available >= 120 {
        available - SPACING
    } else {
        MIN_WIDTH
    };
    let step = if effects.len() > 5 {
        132 / (effects.len() as i32 - 1)
    } else {
        ROW
    };
    let tick_rate = cx.view.tick_rate();
    let (mx, my) = (cx.mouse.0 as i32, cx.mouse.1 as i32);
    let mut y = top;
    for effect in &effects {
        let name = effect_name(effect);
        let name_cells = cx.frame.ui().flatten(&name);
        let duration = duration_text(cx, effect, tick_rate);
        let name_w = cx.frame.ui().width(&name_cells).ceil() as i32;
        let duration_w = cx.text_width(&duration);
        let w = max_width.min((TEXT_X + name_w + SPACING).max(TEXT_X + duration_w + SPACING));
        let sprite = if effect.ambient {
            "container/inventory/effect_background_ambient"
        } else {
            "container/inventory/effect_background"
        };
        cx.sprite(sprite, x0, y, w, 32);
        let max_text = w - TEXT_X - SPACING;
        let compact = if max_text > 0 {
            let clipped = name_w > max_text;
            let cells = if clipped {
                clip(cx, &name_cells, max_text)
            } else {
                name_cells
            };
            cx.frame.cells(
                &cells,
                (x0 + TEXT_X) as f32,
                (y + 7) as f32,
                0xFFFF_FFFF,
                true,
            );
            cx.text(&duration, x0 + TEXT_X, y + 16, 0xFF80_8080, true);
            clipped
        } else {
            true
        };
        if compact && mx >= x0 && mx <= x0 + w && my >= y && my <= y + step {
            let mut lines = TooltipLines::default();
            lines.push_component(name);
            lines.push_text(duration);
            cx.set_tooltip(lines);
        }
        let path = effect
            .effect
            .split_once(':')
            .map_or(&*effect.effect, |p| p.1);
        cx.sprite(&format!("mob_effect/{path}"), x0 + 7, y + 7, 18, 18);
        y += step;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fx(name: &str, duration: i32, ambient: bool) -> ActiveEffect {
        ActiveEffect {
            effect: format!("minecraft:{name}"),
            amplifier: 0,
            duration,
            ambient,
        }
    }

    #[test]
    fn effects_sort_like_vanilla() {
        let mut list = [
            fx("speed", 600, false),
            fx("haste", 100, true),
            fx("strength", -1, false),
            fx("regeneration", 200, false),
        ];
        list.sort_by(effect_order);
        let names: Vec<&str> = list.iter().map(|e| &e.effect[10..]).collect();
        assert_eq!(names, ["regeneration", "speed", "strength", "haste"]);
    }

    #[test]
    fn names_carry_the_level_numeral_up_to_ten() {
        let mut e = fx("speed", 100, false);
        assert!(effect_name(&e).extra.is_empty());
        e.amplifier = 1;
        assert_eq!(effect_name(&e).extra.len(), 2);
        e.amplifier = 10;
        assert!(effect_name(&e).extra.is_empty());
    }

    #[test]
    fn effects_hide_when_the_screen_is_too_narrow() {
        assert!(can_see_effects(320, 176 + 72));
        assert!(!can_see_effects(320, 300));
    }
}
