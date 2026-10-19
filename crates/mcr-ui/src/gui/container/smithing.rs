//! `SmithingScreen` extras: cycling empty-slot icons (`CyclingSlotBackground`) and onboarding tooltips.

use super::screen::ContainerScreen;
use crate::gui::env::{GameView, RenderCx, is_empty};
use crate::gui::tooltip::TooltipLines;
use crate::items::StackView;
use crate::text::parse_legacy;

const CHANGE_TICKS: u32 = 30;
const TRANSITION_TICKS: f32 = 4.0;
const TOOLTIP_WIDTH: f32 = 115.0;

const TEMPLATES: &[&str] = &[
    "container/slot/smithing_template_armor_trim",
    "container/slot/smithing_template_netherite_upgrade",
];
const ARMOR: &[&str] = &[
    "container/slot/helmet",
    "container/slot/chestplate",
    "container/slot/leggings",
    "container/slot/boots",
];
const TRIM_MATERIALS: &[&str] = &[
    "container/slot/ingot",
    "container/slot/redstone_dust",
    "container/slot/lapis_lazuli",
    "container/slot/quartz",
    "container/slot/diamond",
    "container/slot/emerald",
    "container/slot/amethyst_shard",
];
const NETHERITE_BASES: &[&str] = &[
    "container/slot/helmet",
    "container/slot/sword",
    "container/slot/chestplate",
    "container/slot/pickaxe",
    "container/slot/leggings",
    "container/slot/axe",
    "container/slot/boots",
    "container/slot/hoe",
    "container/slot/shovel",
    "container/slot/nautilus_armor",
    "container/slot/spear",
];
const NETHERITE_ADDITIONS: &[&str] = &["container/slot/ingot"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Template {
    ArmorTrim,
    NetheriteUpgrade,
}

impl Template {
    fn of(view: &dyn GameView) -> Option<Self> {
        let stack = view.slot(0).filter(|s| !s.is_empty())?;
        let name = StackView::new(stack)?.name().to_owned();
        match name.as_str() {
            "minecraft:netherite_upgrade_smithing_template" => Some(Self::NetheriteUpgrade),
            n if n.starts_with("minecraft:") && n.ends_with("_armor_trim_smithing_template") => {
                Some(Self::ArmorTrim)
            }
            _ => None,
        }
    }

    fn key(self) -> &'static str {
        match self {
            Self::ArmorTrim => "armor_trim",
            Self::NetheriteUpgrade => "netherite_upgrade",
        }
    }

    fn icons(self) -> (&'static [&'static str], &'static [&'static str]) {
        match self {
            Self::ArmorTrim => (ARMOR, TRIM_MATERIALS),
            Self::NetheriteUpgrade => (NETHERITE_BASES, NETHERITE_ADDITIONS),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct CyclingIcons {
    icons: &'static [&'static str],
    tick: u32,
    index: usize,
}

impl CyclingIcons {
    pub(super) fn tick(&mut self, icons: &'static [&'static str]) {
        if !std::ptr::eq(self.icons, icons) {
            self.icons = icons;
            self.index = 0;
        }
        if !self.icons.is_empty() {
            self.tick = self.tick.wrapping_add(1);
            if self.tick.is_multiple_of(CHANGE_TICKS) {
                self.index = (self.index + 1) % self.icons.len();
            }
        }
    }

    /// The icons to draw with their alpha: the fading previous one, then the current one.
    pub(super) fn layers(&self) -> Vec<(&'static str, f32)> {
        let Some(&current) = self.icons.get(self.index) else {
            return Vec::new();
        };
        let transition = self.icons.len() > 1 && self.tick >= CHANGE_TICKS;
        let alpha = if transition {
            ((self.tick % CHANGE_TICKS) as f32).min(TRANSITION_TICKS) / TRANSITION_TICKS
        } else {
            1.0
        };
        let mut out = Vec::new();
        if alpha < 1.0 {
            let prev = (self.index + self.icons.len() - 1) % self.icons.len();
            out.push((self.icons[prev], 1.0 - alpha));
        }
        out.push((current, alpha));
        out
    }
}

impl ContainerScreen {
    pub(super) fn smithing_tick(&mut self, view: &dyn GameView) {
        let template = Template::of(view);
        let (base, addition) = template.map_or((&[][..], &[][..]), Template::icons);
        self.smithing_icons[0].tick(TEMPLATES);
        self.smithing_icons[1].tick(base);
        self.smithing_icons[2].tick(addition);
    }

    pub(super) fn draw_smithing_icons(&self, cx: &mut RenderCx<'_, '_>) {
        for (slot, icons) in self.smithing_icons.iter().enumerate() {
            if !is_empty(cx.view.slot(slot)) {
                continue;
            }
            let spec = self.layout.slots[slot];
            for (name, alpha) in icons.layers() {
                if let Some(id) = cx.sprite_id(name) {
                    let a = (alpha.clamp(0.0, 1.0) * 255.0).round() as u32;
                    cx.frame.sprite_tinted(
                        id,
                        self.left + spec.x,
                        self.top + spec.y,
                        16,
                        16,
                        a << 24 | 0x00FF_FFFF,
                    );
                }
            }
        }
    }

    /// `SmithingScreen#extractOnboardingTooltips`.
    pub(super) fn smithing_tooltip(&self, cx: &mut RenderCx<'_, '_>) {
        let view = cx.view;
        let mut key = None;
        if view.data(0) == 1 && self.inside([65, 46, 28, 21], self.mouse.0, self.mouse.1) {
            key = Some("container.upgrade.error_tooltip".to_owned());
        }
        if let Some(h) = self.hovered_slot() {
            if is_empty(view.slot(0)) {
                if h == 0 {
                    key = Some("container.upgrade.missing_template_tooltip".to_owned());
                }
            } else if let Some(t) = Template::of(view)
                && is_empty(view.slot(h))
            {
                let which = match h {
                    1 => Some("base_slot_description"),
                    2 => Some("additions_slot_description"),
                    _ => None,
                };
                if let Some(which) = which {
                    key = Some(format!(
                        "item.minecraft.smithing_template.{}.{which}",
                        t.key()
                    ));
                }
            }
        }
        let Some(key) = key else {
            return;
        };
        let text = cx.lang(&key);
        let cells = parse_legacy(&text);
        let mut lines = TooltipLines::default();
        for range in cx.frame.ui().wrap(&cells, TOOLTIP_WIDTH) {
            lines.push_text(cells[range].iter().map(|c| c.ch).collect::<String>());
        }
        cx.set_tooltip(lines);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icons_cycle_every_thirty_ticks_with_a_fade() {
        let mut c = CyclingIcons::default();
        for _ in 0..29 {
            c.tick(ARMOR);
        }
        assert_eq!(c.layers(), [(ARMOR[0], 1.0)]);
        c.tick(ARMOR);
        assert_eq!(c.layers(), [(ARMOR[0], 1.0), (ARMOR[1], 0.0)]);
        c.tick(ARMOR);
        c.tick(ARMOR);
        assert_eq!(c.layers(), [(ARMOR[0], 0.5), (ARMOR[1], 0.5)]);
        for _ in 0..2 {
            c.tick(ARMOR);
        }
        assert_eq!(c.layers(), [(ARMOR[1], 1.0)]);
        c.tick(TRIM_MATERIALS);
        assert_eq!(c.layers()[c.layers().len() - 1].0, TRIM_MATERIALS[0]);
        c.tick(&[]);
        assert!(c.layers().is_empty());
    }
}
