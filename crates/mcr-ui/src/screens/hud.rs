use crate::items::{ItemIcons, ItemView};
use crate::sprites::SpriteId;
use crate::ui::{Ui, UiFrame};

const XP_TEXT: u32 = 0xFF80_FF20;
const HEART_KINDS: [&str; 5] = ["", "poisoned_", "withered_", "frozen_", "absorbing_"];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HeartKind {
    #[default]
    Normal,
    Poisoned,
    Withered,
    Frozen,
}

/// Plain numbers for the survival HUD; health and food are in half units like vanilla.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HudState {
    pub selected_slot: u8,
    pub offhand: bool,
    pub health: i32,
    pub displayed_health: i32,
    pub max_health: i32,
    pub absorption: i32,
    pub heart_kind: HeartKind,
    pub hardcore: bool,
    pub blink: bool,
    /// Seeds vanilla's low-health jitter; `None` when the hearts hold still.
    pub shake_tick: Option<u32>,
    /// Container index lifted by the regeneration wave, or -1.
    pub regen_index: i32,
    pub armor: i32,
    pub food: i32,
    pub hunger_effect: bool,
    pub xp_progress: f32,
    pub xp_level: i32,
    /// False while the locator bar replaces the experience bar; the level number stays.
    pub xp_bar: bool,
}

impl Default for HudState {
    fn default() -> Self {
        Self {
            selected_slot: 0,
            offhand: false,
            health: 20,
            displayed_health: 20,
            max_health: 20,
            absorption: 0,
            heart_kind: HeartKind::Normal,
            hardcore: false,
            blink: false,
            shake_tick: None,
            regen_index: -1,
            armor: 0,
            food: 20,
            hunger_effect: false,
            xp_progress: 0.0,
            xp_level: 0,
            xp_bar: true,
        }
    }
}

#[derive(Debug)]
pub struct HudSprites {
    hotbar: Option<SpriteId>,
    selection: Option<SpriteId>,
    offhand_left: Option<SpriteId>,
    armor: [Option<SpriteId>; 3],
    food: [[Option<SpriteId>; 3]; 2],
    xp_background: Option<SpriteId>,
    xp_progress: Option<SpriteId>,
    heart_container: [[Option<SpriteId>; 2]; 2],
    hearts: Vec<Option<SpriteId>>,
}

fn heart_index(kind: usize, hardcore: bool, half: bool, blink: bool) -> usize {
    ((kind * 2 + usize::from(hardcore)) * 2 + usize::from(half)) * 2 + usize::from(blink)
}

impl HudSprites {
    pub fn new(ui: &Ui) -> Self {
        let id = |name: &str| ui.sprite_id(name);
        let mut hearts = vec![None; HEART_KINDS.len() * 8];
        for (k, prefix) in HEART_KINDS.iter().enumerate() {
            for (hardcore, half, blink) in all_bools() {
                let name = format!(
                    "hud/heart/{prefix}{}{}{}",
                    if hardcore { "hardcore_" } else { "" },
                    if half { "half" } else { "full" },
                    if blink { "_blinking" } else { "" }
                );
                hearts[heart_index(k, hardcore, half, blink)] = id(&name);
            }
        }
        let container = |hardcore: bool, blink: bool| {
            id(&format!(
                "hud/heart/container{}{}",
                if hardcore { "_hardcore" } else { "" },
                if blink { "_blinking" } else { "" }
            ))
        };
        Self {
            hotbar: id("hud/hotbar"),
            selection: id("hud/hotbar_selection"),
            offhand_left: id("hud/hotbar_offhand_left"),
            armor: ["empty", "half", "full"].map(|s| id(&format!("hud/armor_{s}"))),
            food: [false, true].map(|hunger| {
                ["empty", "half", "full"].map(|s| {
                    id(&format!(
                        "hud/food_{s}{}",
                        if hunger { "_hunger" } else { "" }
                    ))
                })
            }),
            xp_background: id("hud/experience_bar_background"),
            xp_progress: id("hud/experience_bar_progress"),
            heart_container: [
                [container(false, false), container(false, true)],
                [container(true, false), container(true, true)],
            ],
            hearts,
        }
    }
}

/// Items shown on the hotbar, in slot order, and in the offhand slot.
#[derive(Debug, Default)]
pub struct HudItems<'a> {
    pub hotbar: [Option<ItemView<'a>>; 9],
    pub offhand: Option<ItemView<'a>>,
}

impl HudSprites {
    /// Draws hotbar and offhand items over the hotbar background.
    pub fn draw_items(&self, frame: &mut UiFrame<'_>, icons: &mut ItemIcons, items: &HudItems) {
        let (gui_w, gui_h) = frame.gui_size();
        let y = gui_h - 16 - 3;
        let left = gui_w / 2 - 90;
        for (slot, view) in items.hotbar.iter().enumerate() {
            if let Some(view) = view {
                icons.draw_with_decorations(frame, view, left + slot as i32 * 20 + 2, y);
            }
        }
        if let Some(view) = &items.offhand {
            icons.draw_with_decorations(frame, view, gui_w / 2 - 91 - 26, y);
        }
    }
}

fn all_bools() -> impl Iterator<Item = (bool, bool, bool)> {
    (0..8).map(|i| (i & 4 != 0, i & 2 != 0, i & 1 != 0))
}

impl HudSprites {
    pub fn draw(&self, frame: &mut UiFrame<'_>, state: &HudState) {
        let (gui_w, gui_h) = frame.gui_size();
        let center = gui_w / 2;
        let left = center - 91;
        let right = center + 91;
        if let Some(s) = self.hotbar {
            frame.sprite(s, left, gui_h - 22, 182, 22);
        }
        if let Some(s) = self.selection {
            frame.sprite(
                s,
                left - 1 + i32::from(state.selected_slot) * 20,
                gui_h - 23,
                24,
                23,
            );
        }
        if let (true, Some(s)) = (state.offhand, self.offhand_left) {
            frame.sprite(s, left - 29, gui_h - 23, 29, 24);
        }
        self.draw_xp(frame, state);
        self.draw_status(frame, state, left, right, gui_h - 39);
    }

    fn draw_xp(&self, frame: &mut UiFrame<'_>, state: &HudState) {
        let (gui_w, gui_h) = frame.gui_size();
        let (x, y) = ((gui_w - 182) / 2, gui_h - 24 - 5);
        let progress = (state.xp_progress * 183.0) as i32;
        if let (true, Some(bg)) = (state.xp_bar, self.xp_background) {
            frame.sprite(bg, x, y, 182, 5);
        }
        if let (Some(fg), true, true) = (self.xp_progress, progress > 0, state.xp_bar) {
            frame.sprite_region(fg, (0, 0), (x, y, progress.min(182), 5));
        }
        if state.xp_level > 0 {
            let text = state.xp_level.to_string();
            let width = frame.ui().text_width(&text);
            let tx = ((gui_w as f32 - width) / 2.0).floor();
            let ty = (gui_h - 24 - 9 - 2) as f32;
            for (dx, dy) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
                frame.text(&text, tx + dx, ty + dy, 0xFF00_0000, false);
            }
            frame.text(&text, tx, ty, XP_TEXT, false);
        }
    }

    fn draw_status(
        &self,
        frame: &mut UiFrame<'_>,
        s: &HudState,
        left: i32,
        right: i32,
        base_y: i32,
    ) {
        // Server-controlled values; clamp to vanilla's attribute maxima so loops stay bounded.
        let max_health = s
            .max_health
            .max(s.displayed_health)
            .max(s.health)
            .clamp(0, 2048);
        let absorption = s.absorption.clamp(0, 4096);
        let s = &HudState { absorption, ..*s };
        let rows = (max_health + s.absorption + 19) / 20;
        let row_height = (10 - (rows - 2)).max(3);
        if s.armor > 0 {
            let y = base_y - (rows - 1) * row_height - 10;
            for i in 0..10 {
                let state = match i * 2 + 1 {
                    n if n < s.armor => 2,
                    n if n == s.armor => 1,
                    _ => 0,
                };
                if let Some(sprite) = self.armor[state] {
                    frame.sprite(sprite, left + i * 8, y, 9, 9);
                }
            }
        }
        let containers = (max_health + 1) / 2;
        let absorption_containers = (s.absorption + 1) / 2;
        let kind = s.heart_kind as usize;
        let mut shake = s.shake_tick.map(JavaLcg::seeded);
        for index in (0..containers + absorption_containers).rev() {
            let (row, column) = (index / 10, index % 10);
            let (x, mut y) = (left + column * 8, base_y - row * row_height);
            if let Some(lcg) = &mut shake {
                y += lcg.next_bit();
            }
            if index < containers && index == s.regen_index {
                y -= 2;
            }
            if let Some(c) = self.heart_container[usize::from(s.hardcore)][usize::from(s.blink)] {
                frame.sprite(c, x, y, 9, 9);
            }
            let halves = index * 2;
            if index >= containers {
                let absorbed = halves - containers * 2;
                if absorbed < s.absorption {
                    let k = if kind == HeartKind::Withered as usize {
                        kind
                    } else {
                        4
                    };
                    self.heart(
                        frame,
                        k,
                        s.hardcore,
                        absorbed + 1 == s.absorption,
                        false,
                        x,
                        y,
                    );
                }
            }
            if s.blink && halves < s.displayed_health {
                self.heart(
                    frame,
                    kind,
                    s.hardcore,
                    halves + 1 == s.displayed_health,
                    true,
                    x,
                    y,
                );
            }
            if halves < s.health {
                self.heart(frame, kind, s.hardcore, halves + 1 == s.health, false, x, y);
            }
        }
        let food = &self.food[usize::from(s.hunger_effect)];
        for i in 0..10 {
            let x = right - i * 8 - 9;
            if let Some(e) = food[0] {
                frame.sprite(e, x, base_y, 9, 9);
            }
            let state = match i * 2 + 1 {
                n if n < s.food => 2,
                n if n == s.food => 1,
                _ => 0,
            };
            if let (true, Some(f)) = (state > 0, food[state]) {
                frame.sprite(f, x, base_y, 9, 9);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn heart(
        &self,
        frame: &mut UiFrame<'_>,
        kind: usize,
        hardcore: bool,
        half: bool,
        blink: bool,
        x: i32,
        y: i32,
    ) {
        if let Some(s) = self.hearts[heart_index(kind, hardcore, half, blink)] {
            frame.sprite(s, x, y, 9, 9);
        }
    }
}

/// Java's `Random` reduced to what `nextInt(2)` needs, so the shake matches vanilla frame for frame.
struct JavaLcg(i64);

impl JavaLcg {
    fn seeded(tick: u32) -> Self {
        let seed = i64::from((tick as i32).wrapping_mul(312_871));
        Self((seed ^ 0x5_DEEC_E66D) & ((1 << 48) - 1))
    }

    fn next_bit(&mut self) -> i32 {
        self.0 = (self.0.wrapping_mul(0x5_DEEC_E66D).wrapping_add(0xB)) & ((1 << 48) - 1);
        ((self.0 >> 17) >> 30) as i32 & 1
    }
}
