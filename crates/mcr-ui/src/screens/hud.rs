use crate::items::{ItemIcons, ItemView};
use crate::sprites::SpriteId;
use crate::ui::{Blend, Ui, UiFrame};

const XP_TEXT: u32 = 0xFF80_FF20;
const HEART_KINDS: [&str; 5] = ["", "poisoned_", "withered_", "frozen_", "absorbing_"];
const AIR_BUBBLES: i32 = 10;
/// `Hud#getVehicleMaxHearts` caps mounts at three rows.
pub const MAX_VEHICLE_HEARTS: i32 = 30;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HeartKind {
    #[default]
    Normal,
    Poisoned,
    Withered,
    Frozen,
}

/// The bar above the hotbar, vanilla's `Hud.ContextualInfo`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum ContextualBar {
    Empty,
    #[default]
    Experience,
    Locator,
    /// A jumpable mount's charge (0..1), or its dash cooldown.
    Jump {
        scale: f32,
        cooldown: bool,
    },
}

impl ContextualBar {
    /// `Hud#nextContextualInfoState`. `jump` is the mount's charge and cooldown, `experience`
    /// whether the game mode has experience and `xp_recent` the 100 ticks after an XP change.
    pub fn choose(
        waypoints: bool,
        jump: Option<(f32, bool)>,
        experience: bool,
        xp_recent: bool,
    ) -> Self {
        let jump_bar = jump.map(|(scale, cooldown)| Self::Jump { scale, cooldown });
        if waypoints {
            match jump_bar {
                Some(bar @ Self::Jump { scale, cooldown }) if scale > 0.0 || cooldown => bar,
                _ if experience && xp_recent => Self::Experience,
                _ => Self::Locator,
            }
        } else if let Some(bar) = jump_bar {
            bar
        } else if experience {
            Self::Experience
        } else {
            Self::Empty
        }
    }
}

/// Air supply in ticks and whether the eyes are under water.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AirState {
    pub supply: i32,
    pub max: i32,
    pub underwater: bool,
}

impl Default for AirState {
    fn default() -> Self {
        Self {
            supply: 300,
            max: 300,
            underwater: false,
        }
    }
}

/// Bubble counts of `Hud#extractAirBubbles`, each 1-based from the right.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AirBubbles {
    pub full: i32,
    pub popping: i32,
    pub empty: i32,
}

impl AirState {
    fn max(&self) -> i32 {
        self.max.max(1)
    }

    fn supply(&self) -> i32 {
        self.supply.clamp(0, self.max())
    }

    pub fn visible(&self) -> bool {
        self.underwater || self.supply() < self.max()
    }

    fn bubble(&self, offset: i32) -> i32 {
        ((self.supply() + offset) as f32 * AIR_BUBBLES as f32 / self.max() as f32).ceil() as i32
    }

    pub fn bubbles(&self) -> AirBubbles {
        let delay = i32::from(self.supply() != 0 && self.underwater);
        AirBubbles {
            full: self.bubble(-2),
            popping: self.bubble(0),
            empty: AIR_BUBBLES - self.bubble(delay),
        }
    }

    /// The bubble bursting this frame, which vanilla answers with `ui.hud.bubble_pop`.
    pub fn popping_bubble(&self) -> Option<i32> {
        let b = self.bubbles();
        (self.visible()
            && self.underwater
            && b.full != b.popping
            && (1..=AIR_BUBBLES).contains(&b.popping))
        .then_some(b.popping)
    }

    /// `Hud#playAirBubblePoppedSound`: volume and pitch rise as the empty bubbles add up.
    pub fn pop_sound(&self) -> (f32, f32) {
        let empty = self.bubbles().empty;
        (
            0.5 + 0.1 * (empty - 3 + 1).max(0) as f32,
            1.0 + 0.1 * (empty - 5 + 1).max(0) as f32,
        )
    }
}

/// Plain numbers for the survival HUD; health and food are in half units like vanilla.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HudState {
    /// Vanilla's GUI tick: seeds the jitter of hearts, food and empty bubbles.
    pub tick: u32,
    pub selected_slot: u8,
    pub offhand: bool,
    /// Left main arm: the offhand slot and the hotbar attack indicator swap sides.
    pub left_handed: bool,
    /// Hotbar attack indicator fill while the attack is still charging.
    pub hotbar_attack: Option<f32>,
    /// False in spectator, whose menu replaces the hotbar.
    pub hotbar: bool,
    /// Armour, hearts, food and air (`canHurtPlayer`).
    pub player_health: bool,
    pub health: i32,
    pub displayed_health: i32,
    pub max_health: i32,
    pub absorption: i32,
    pub heart_kind: HeartKind,
    pub hardcore: bool,
    pub blink: bool,
    /// Low-health jitter of the hearts.
    pub shake: bool,
    /// Container index lifted by the regeneration wave, or -1.
    pub regen_index: i32,
    pub armor: i32,
    pub food: i32,
    pub hunger_effect: bool,
    /// Zero saturation makes the food icons jitter.
    pub saturation_empty: bool,
    pub air: AirState,
    /// Hearts of a living mount (0 without one); they replace the food row.
    pub vehicle_hearts: i32,
    pub vehicle_health: i32,
    pub contextual: ContextualBar,
    pub xp_progress: f32,
    pub xp_level: i32,
    /// The game mode has experience, so the level number shows.
    pub experience: bool,
}

impl Default for HudState {
    fn default() -> Self {
        Self {
            tick: 0,
            selected_slot: 0,
            offhand: false,
            left_handed: false,
            hotbar_attack: None,
            hotbar: true,
            player_health: true,
            health: 20,
            displayed_health: 20,
            max_health: 20,
            absorption: 0,
            heart_kind: HeartKind::Normal,
            hardcore: false,
            blink: false,
            shake: false,
            regen_index: -1,
            armor: 0,
            food: 20,
            hunger_effect: false,
            saturation_empty: false,
            air: AirState::default(),
            vehicle_hearts: 0,
            vehicle_health: 0,
            contextual: ContextualBar::Experience,
            xp_progress: 0.0,
            xp_level: 0,
            experience: true,
        }
    }
}

/// The crosshair's attack indicator in `attackIndicator: crosshair` mode.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum CrosshairIndicator {
    #[default]
    None,
    /// A fully charged attack on a living target in reach.
    Full,
    /// The charge while it is below 1.
    Progress(f32),
}

/// One status effect icon, in vanilla's drawing order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HudEffect {
    pub sprite: Option<SpriteId>,
    pub beneficial: bool,
    pub ambient: bool,
    /// Ticks left, -1 for infinite.
    pub duration: i32,
}

/// Icon opacity of `Hud#extractEffects`: effects blink through their last ten seconds.
pub fn effect_alpha(duration: i32, ambient: bool) -> f32 {
    if ambient || duration == -1 || duration > 200 {
        return 1.0;
    }
    let used_seconds = 10 - duration / 20;
    let alpha = (duration as f32 / 10.0 / 5.0 * 0.5).clamp(0.0, 0.5)
        + (duration as f32 * std::f32::consts::PI / 5.0).cos()
            * (used_seconds as f32 / 10.0 * 0.25).clamp(0.0, 0.25);
    alpha.clamp(0.0, 1.0)
}

#[derive(Debug)]
struct ExtraSprites {
    crosshair: Option<SpriteId>,
    crosshair_full: Option<SpriteId>,
    crosshair_background: Option<SpriteId>,
    crosshair_progress: Option<SpriteId>,
    hotbar_attack_background: Option<SpriteId>,
    hotbar_attack_progress: Option<SpriteId>,
    offhand_right: Option<SpriteId>,
    effect_background: Option<SpriteId>,
    effect_background_ambient: Option<SpriteId>,
    air: [Option<SpriteId>; 3],
    vehicle: [Option<SpriteId>; 3],
    jump: [Option<SpriteId>; 3],
    locator_background: Option<SpriteId>,
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
    extra: ExtraSprites,
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
            extra: ExtraSprites {
                crosshair: id("hud/crosshair"),
                crosshair_full: id("hud/crosshair_attack_indicator_full"),
                crosshair_background: id("hud/crosshair_attack_indicator_background"),
                crosshair_progress: id("hud/crosshair_attack_indicator_progress"),
                hotbar_attack_background: id("hud/hotbar_attack_indicator_background"),
                hotbar_attack_progress: id("hud/hotbar_attack_indicator_progress"),
                offhand_right: id("hud/hotbar_offhand_right"),
                effect_background: id("hud/effect_background"),
                effect_background_ambient: id("hud/effect_background_ambient"),
                air: ["air", "air_bursting", "air_empty"].map(|s| id(&format!("hud/{s}"))),
                vehicle: ["container", "full", "half"]
                    .map(|s| id(&format!("hud/heart/vehicle_{s}"))),
                jump: ["background", "cooldown", "progress"]
                    .map(|s| id(&format!("hud/jump_bar_{s}"))),
                locator_background: id("hud/locator_bar_background"),
            },
        }
    }
}

/// Items shown on the hotbar, in slot order, and in the offhand slot.
#[derive(Debug, Default)]
pub struct HudItems<'a> {
    pub hotbar: [Option<ItemView<'a>>; 9],
    pub offhand: Option<ItemView<'a>>,
    /// `ItemStack#getPopTime` minus the partial tick, per hotbar slot.
    pub pop: [f32; 9],
    pub offhand_pop: f32,
    pub left_handed: bool,
}

impl HudSprites {
    /// Draws hotbar and offhand items over the hotbar background.
    pub fn draw_items(&self, frame: &mut UiFrame<'_>, icons: &mut ItemIcons, items: &HudItems) {
        let (gui_w, gui_h) = frame.gui_size();
        let y = gui_h - 16 - 3;
        let left = gui_w / 2 - 90;
        for (slot, view) in items.hotbar.iter().enumerate() {
            if let Some(view) = view {
                draw_slot(
                    frame,
                    icons,
                    view,
                    left + slot as i32 * 20 + 2,
                    y,
                    items.pop[slot],
                );
            }
        }
        if let Some(view) = &items.offhand {
            let x = if items.left_handed {
                gui_w / 2 + 91 + 10
            } else {
                gui_w / 2 - 91 - 26
            };
            draw_slot(frame, icons, view, x, y, items.offhand_pop);
        }
    }
}

/// `Hud#extractSlot`: a stack that just grew is squeezed for five ticks.
fn draw_slot(
    frame: &mut UiFrame<'_>,
    icons: &mut ItemIcons,
    view: &ItemView,
    x: i32,
    y: i32,
    pop: f32,
) {
    if pop > 0.0 {
        let squeeze = 1.0 + pop / 5.0;
        icons.draw_squeezed(frame, view, x, y, 1.0 / squeeze, (squeeze + 1.0) / 2.0);
    } else {
        icons.draw(frame, view, x, y);
    }
    icons.decorations(frame, view, x, y);
}

fn all_bools() -> impl Iterator<Item = (bool, bool, bool)> {
    (0..8).map(|i| (i & 4 != 0, i & 2 != 0, i & 1 != 0))
}

fn white(alpha: f32) -> u32 {
    ((alpha.clamp(0.0, 1.0) * 255.0) as u32) << 24 | 0xFF_FFFF
}

impl HudSprites {
    /// Everything but the hotbar items, the locator dots and the text lines.
    pub fn draw(&self, frame: &mut UiFrame<'_>, state: &HudState) {
        self.draw_hotbar(frame, state);
        self.draw_status(frame, state);
    }

    /// Hotbar, selection, offhand slot and the hotbar attack indicator.
    pub fn draw_hotbar(&self, frame: &mut UiFrame<'_>, state: &HudState) {
        if !state.hotbar {
            return;
        }
        let (gui_w, gui_h) = frame.gui_size();
        let center = gui_w / 2;
        let left = center - 91;
        if let Some(s) = self.hotbar {
            frame.sprite(s, left, gui_h - 22, 182, 22);
        }
        if let Some(s) = self.selection {
            frame.sprite(
                s,
                left - 1 + i32::from(state.selected_slot.min(8)) * 20,
                gui_h - 23,
                24,
                23,
            );
        }
        if state.offhand {
            let slot = if state.left_handed {
                self.extra.offhand_right.map(|s| (s, center + 91))
            } else {
                self.offhand_left.map(|s| (s, left - 29))
            };
            if let Some((s, x)) = slot {
                frame.sprite(s, x, gui_h - 23, 29, 24);
            }
        }
        if let Some(scale) = state.hotbar_attack.filter(|s| *s < 1.0) {
            let y = gui_h - 20;
            let x = if state.left_handed {
                center - 91 - 22
            } else {
                center + 91 + 6
            };
            let progress = (scale.clamp(0.0, 1.0) * 19.0) as i32;
            if let Some(bg) = self.extra.hotbar_attack_background {
                frame.sprite(bg, x, y, 18, 18);
            }
            if let (Some(fg), true) = (self.extra.hotbar_attack_progress, progress > 0) {
                frame.sprite_region(fg, (0, 18 - progress), (x, y + 18 - progress, 18, progress));
            }
        }
    }

    /// Player health block, mount hearts, the contextual bar's background and the level number,
    /// in vanilla's order; the caller draws the locator dots after this.
    pub fn draw_status(&self, frame: &mut UiFrame<'_>, state: &HudState) {
        let (gui_w, gui_h) = frame.gui_size();
        let center = gui_w / 2;
        let mut lcg = JavaLcg::seeded(state.tick);
        if state.player_health {
            self.draw_player_health(frame, state, center - 91, center + 91, gui_h - 39, &mut lcg);
        }
        self.draw_vehicle_health(frame, state, center + 91, gui_h - 39);
        self.draw_contextual(frame, state);
        if state.experience && state.xp_level > 0 {
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

    fn draw_contextual(&self, frame: &mut UiFrame<'_>, state: &HudState) {
        let (gui_w, gui_h) = frame.gui_size();
        let (x, y) = ((gui_w - 182) / 2, gui_h - 24 - 5);
        match state.contextual {
            ContextualBar::Empty => {}
            ContextualBar::Experience => {
                let progress = (state.xp_progress.clamp(0.0, 1.0) * 183.0) as i32;
                if let Some(bg) = self.xp_background {
                    frame.sprite(bg, x, y, 182, 5);
                }
                if let (Some(fg), true) = (self.xp_progress, progress > 0) {
                    frame.sprite_region(fg, (0, 0), (x, y, progress.min(182), 5));
                }
            }
            ContextualBar::Locator => {
                if let Some(bg) = self.extra.locator_background {
                    frame.sprite(bg, x, y, 182, 5);
                }
            }
            ContextualBar::Jump { scale, cooldown } => {
                let [bg, cool, fg] = self.extra.jump;
                if let Some(bg) = bg {
                    frame.sprite(bg, x, y, 182, 5);
                }
                if cooldown {
                    if let Some(c) = cool {
                        frame.sprite(c, x, y, 182, 5);
                    }
                } else {
                    let scale = scale.clamp(0.0, 1.0);
                    let progress = (scale * 181.0).floor() as i32 + i32::from(scale > 0.0);
                    if let (Some(fg), true) = (fg, progress > 0) {
                        frame.sprite_region(fg, (0, 0), (x, y, progress, 5));
                    }
                }
            }
        }
    }

    fn draw_vehicle_health(&self, frame: &mut UiFrame<'_>, s: &HudState, right: i32, base_y: i32) {
        let mut hearts = s.vehicle_hearts.clamp(0, MAX_VEHICLE_HEARTS);
        let [container, full, half] = self.extra.vehicle;
        let mut y = base_y;
        let mut base_health = 0;
        while hearts > 0 {
            let row = hearts.min(10);
            hearts -= row;
            for i in 0..row {
                let x = right - i * 8 - 9;
                if let Some(c) = container {
                    frame.sprite(c, x, y, 9, 9);
                }
                let n = i * 2 + 1 + base_health;
                let fill = if n < s.vehicle_health {
                    full
                } else if n == s.vehicle_health {
                    half
                } else {
                    None
                };
                if let Some(f) = fill {
                    frame.sprite(f, x, y, 9, 9);
                }
            }
            y -= 10;
            base_health += 20;
        }
    }

    fn draw_player_health(
        &self,
        frame: &mut UiFrame<'_>,
        s: &HudState,
        left: i32,
        right: i32,
        base_y: i32,
        lcg: &mut JavaLcg,
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
        for index in (0..containers + absorption_containers).rev() {
            let (row, column) = (index / 10, index % 10);
            let (x, mut y) = (left + column * 8, base_y - row * row_height);
            if s.shake {
                y += lcg.next_int(2);
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
        let vehicle_hearts = s.vehicle_hearts.clamp(0, MAX_VEHICLE_HEARTS);
        if vehicle_hearts == 0 {
            let food = &self.food[usize::from(s.hunger_effect)];
            let level = s.food.clamp(0, 20);
            let jitter = s.saturation_empty && s.tick.is_multiple_of(level as u32 * 3 + 1);
            for i in 0..10 {
                let mut y = base_y;
                if jitter {
                    y += lcg.next_int(3) - 1;
                }
                let x = right - i * 8 - 9;
                if let Some(e) = food[0] {
                    frame.sprite(e, x, y, 9, 9);
                }
                let state = match i * 2 + 1 {
                    n if n < level => 2,
                    n if n == level => 1,
                    _ => 0,
                };
                if let (true, Some(f)) = (state > 0, food[state]) {
                    frame.sprite(f, x, y, 9, 9);
                }
            }
        }
        self.draw_air(frame, s, right, base_y, vehicle_hearts, lcg);
    }

    fn draw_air(
        &self,
        frame: &mut UiFrame<'_>,
        s: &HudState,
        right: i32,
        base_y: i32,
        vehicle_hearts: i32,
        lcg: &mut JavaLcg,
    ) {
        let air = s.air;
        if !air.visible() {
            return;
        }
        let rows = (vehicle_hearts + 9) / 10;
        let y = base_y - 10 - (rows.max(1) - 1) * 10;
        let b = air.bubbles();
        let popping = b.full != b.popping;
        let [full, bursting, empty] = self.extra.air;
        for bubble in 1..=AIR_BUBBLES {
            let x = right - (bubble - 1) * 8 - 9;
            if bubble <= b.full {
                if let Some(f) = full {
                    frame.sprite(f, x, y, 9, 9);
                }
            } else if popping && bubble == b.popping && air.underwater {
                if let Some(p) = bursting {
                    frame.sprite(p, x, y, 9, 9);
                }
            } else if bubble > AIR_BUBBLES - b.empty {
                let wobble = if b.empty == AIR_BUBBLES && s.tick.is_multiple_of(2) {
                    lcg.next_int(2)
                } else {
                    0
                };
                if let Some(e) = empty {
                    frame.sprite(e, x, y + wobble, 9, 9);
                }
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

    /// `Hud#extractCrosshair` once vanilla decided to draw it, with the inverting blend.
    pub fn draw_crosshair(&self, frame: &mut UiFrame<'_>, indicator: CrosshairIndicator) {
        let (gui_w, gui_h) = frame.gui_size();
        frame.set_blend(Blend::Invert);
        if let Some(c) = self.extra.crosshair {
            frame.sprite(c, (gui_w - 15) / 2, (gui_h - 15) / 2, 15, 15);
        }
        let (x, y) = (gui_w / 2 - 8, gui_h / 2 - 7 + 16);
        match indicator {
            CrosshairIndicator::None => {}
            CrosshairIndicator::Full => {
                if let Some(f) = self.extra.crosshair_full {
                    frame.sprite(f, x, y, 16, 16);
                }
            }
            CrosshairIndicator::Progress(scale) if scale < 1.0 => {
                let progress = (scale.clamp(0.0, 1.0) * 17.0) as i32;
                if let Some(bg) = self.extra.crosshair_background {
                    frame.sprite(bg, x, y, 16, 4);
                }
                if let (Some(fg), true) = (self.extra.crosshair_progress, progress > 0) {
                    frame.sprite_region(fg, (0, 0), (x, y, progress, 4));
                }
            }
            CrosshairIndicator::Progress(_) => {}
        }
        frame.set_blend(Blend::Normal);
    }

    /// `Hud#extractEffects`: beneficial icons along the top right, the others in a row below.
    pub fn draw_effects(&self, frame: &mut UiFrame<'_>, effects: &[HudEffect]) {
        let (gui_w, _) = frame.gui_size();
        let (mut beneficial, mut harmful) = (0, 0);
        for e in effects {
            let (mut x, mut y) = (gui_w, 1);
            if e.beneficial {
                beneficial += 1;
                x -= 25 * beneficial;
            } else {
                harmful += 1;
                x -= 25 * harmful;
                y += 26;
            }
            let background = if e.ambient {
                self.extra.effect_background_ambient
            } else {
                self.extra.effect_background
            };
            if let Some(bg) = background {
                frame.sprite(bg, x, y, 24, 24);
            }
            if let Some(icon) = e.sprite {
                let alpha = effect_alpha(e.duration, e.ambient);
                frame.sprite_tinted(icon, x + 3, y + 3, 18, 18, white(alpha));
            }
        }
    }
}

/// Java's `Random` reduced to `nextInt`, so the jitter matches vanilla frame for frame.
struct JavaLcg(i64);

impl JavaLcg {
    fn seeded(tick: u32) -> Self {
        let seed = i64::from((tick as i32).wrapping_mul(312_871));
        Self((seed ^ 0x5_DEEC_E66D) & ((1 << 48) - 1))
    }

    fn next31(&mut self) -> i32 {
        self.0 = (self.0.wrapping_mul(0x5_DEEC_E66D).wrapping_add(0xB)) & ((1 << 48) - 1);
        (self.0 >> 17) as i32
    }

    fn next_int(&mut self, bound: i32) -> i32 {
        let mut r = self.next31();
        let m = bound - 1;
        if bound & m == 0 {
            return ((i64::from(bound) * i64::from(r)) >> 31) as i32;
        }
        let mut u = r;
        loop {
            r = u % bound;
            if u - r + m >= 0 {
                return r;
            }
            u = self.next31();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_random_matches_known_sequence() {
        let mut lcg = JavaLcg(0);
        lcg.0 = (42 ^ 0x5_DEEC_E66D) & ((1 << 48) - 1);
        let ints: Vec<i32> = (0..5).map(|_| lcg.next_int(3)).collect();
        assert_eq!(ints, [2, 0, 0, 2, 0]);
        let mut hud = JavaLcg::seeded(7);
        let bits: Vec<i32> = (0..5).map(|_| hud.next_int(2)).collect();
        assert_eq!(bits, [1, 1, 1, 0, 0]);
    }

    #[test]
    fn air_bubbles_follow_vanilla_counts() {
        let full = AirState::default();
        assert!(!full.visible());
        let diving = AirState {
            underwater: true,
            ..full
        };
        assert!(diving.visible());
        assert_eq!(
            diving.bubbles(),
            AirBubbles {
                full: 10,
                popping: 10,
                empty: -1
            }
        );
        let low = AirState {
            supply: 29,
            max: 300,
            underwater: true,
        };
        assert_eq!(
            low.bubbles(),
            AirBubbles {
                full: 1,
                popping: 1,
                empty: 9
            }
        );
        let bursting = AirState { supply: 31, ..low };
        assert_eq!(bursting.popping_bubble(), Some(2));
        assert_eq!(
            AirState {
                underwater: false,
                ..bursting
            }
            .popping_bubble(),
            None
        );
        let out = AirState {
            supply: 150,
            max: 300,
            underwater: false,
        };
        assert!(out.visible());
        assert_eq!(out.bubbles().empty, 5);
        assert_eq!(low.pop_sound(), (1.2, 1.5));
    }

    #[test]
    fn contextual_bar_follows_vanilla_priorities() {
        use ContextualBar as C;
        assert_eq!(C::choose(false, None, true, false), C::Experience);
        assert_eq!(C::choose(false, None, false, false), C::Empty);
        assert_eq!(C::choose(true, None, false, false), C::Locator);
        assert_eq!(C::choose(true, None, true, true), C::Experience);
        assert_eq!(
            C::choose(false, Some((0.0, false)), true, true),
            C::Jump {
                scale: 0.0,
                cooldown: false
            }
        );
        assert_eq!(C::choose(true, Some((0.0, false)), true, false), C::Locator);
        assert_eq!(
            C::choose(true, Some((0.5, false)), true, true),
            C::Jump {
                scale: 0.5,
                cooldown: false
            }
        );
    }

    #[test]
    fn effects_blink_in_their_last_ten_seconds() {
        assert_eq!(effect_alpha(201, false), 1.0);
        assert_eq!(effect_alpha(-1, false), 1.0);
        assert_eq!(effect_alpha(100, true), 1.0);
        assert!(effect_alpha(10, false) < 0.5);
        assert!((effect_alpha(200, false) - 0.5).abs() < 0.3);
    }
}
