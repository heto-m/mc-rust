use std::cell::RefCell;
use std::time::Instant;

use mcr_client::spectator::{MenuItem, SpectatorMenu};
use mcr_client::{ClientState, ScoreStyle, boss_bars_ordered, tab_name};
use mcr_data::Version;
use mcr_protocol::play::clientbound::ui::{BossBarColor, BossBarOverlay, ObjectiveRenderType};
use mcr_protocol::types::entity_data::EntityDataValue;
use mcr_protocol::types::game::GameType;
use mcr_protocol::types::text::Arg;
use mcr_protocol::types::text::Component;
use mcr_settings::{InputKey, Options};
use mcr_ui::{
    AirState, ChatOptions, ContextualBar, CrosshairIndicator, DebugOverlay, EffectCategory,
    EffectSortKey, HeartKind, HudEffect, HudItems, HudSprites, HudState, ItemIcons, ItemRef,
    MAX_VEHICLE_HEARTS, ScreenAssets, SharedChat, SpriteId, StackView, Ui, UiFrame,
};
use rustc_hash::FxHashMap;

use crate::skins::{HEAD_SIZE, SkinStore};

const MAX_HEAD_SPRITES: usize = 256;
const MAX_AIR: i32 = 300;
const MAX_HUD_EFFECTS: usize = 64;
const MAX_TAB_HEALTH: usize = 1024;
const AIR_DATA_INDEX: u8 = 1;
pub(crate) const HEALTH_DATA_INDEX: u8 = 9;
const EFFECT_FLAG_AMBIENT: u8 = 1;
const EFFECT_FLAG_SHOW_ICON: u8 = 4;
const ABSORPTION_DATA_INDEX: u8 = 17;
const FROZEN_DATA_INDEX: u8 = 7;
const FULLY_FROZEN_TICKS: i32 = 140;
const LIST_SLOT: u8 = 0;
const OFFHAND_SLOT: usize = 45;
const SPECTATOR_NAME: u32 = 0x90FF_FFFF;
const TAB_BACKGROUND: u32 = 0x8000_0000;
const TAB_SLOT: u32 = 0x20FF_FFFF;
const TAB_COLUMN_GAP: i32 = 5;

#[derive(Debug, Clone)]
struct WaypointStyle {
    near: f32,
    far: f32,
    sprites: Vec<String>,
}

impl WaypointStyle {
    fn fallback() -> Self {
        Self {
            near: 128.0,
            far: 332.0,
            sprites: (0..4).map(|i| format!("default_{i}")).collect(),
        }
    }

    fn parse(json: &[u8]) -> Option<Self> {
        let v: serde_json::Value = serde_json::from_slice(json).ok()?;
        let num = |k: &str, d: f64| v.get(k).and_then(|n| n.as_f64()).unwrap_or(d);
        let sprites: Vec<String> = v
            .get("sprites")?
            .as_array()?
            .iter()
            .take(16)
            .filter_map(|s| {
                let s = s.as_str()?;
                let name = s.strip_prefix("minecraft:").unwrap_or(s);
                name.bytes()
                    .all(|b| {
                        b.is_ascii_lowercase()
                            || b.is_ascii_digit()
                            || matches!(b, b'_' | b'/' | b'-')
                    })
                    .then(|| name.to_owned())
            })
            .collect();
        let (near, far) = (
            num("near_distance", 128.0) as f32,
            num("far_distance", 332.0) as f32,
        );
        (!sprites.is_empty() && near > 0.0 && near < far).then_some(Self { near, far, sprites })
    }
}

#[derive(Debug)]
struct Sprites {
    hotbar: Option<SpriteId>,
    selection: Option<SpriteId>,
    ping: [Option<SpriteId>; 6],
    tab_hearts: TabHeartSprites,
    arrow_up: Option<SpriteId>,
    arrow_down: Option<SpriteId>,
    spectator: SpectatorSprites,
}

#[derive(Debug)]
struct TabHeartSprites {
    container: Option<SpriteId>,
    container_blinking: Option<SpriteId>,
    full: Option<SpriteId>,
    full_blinking: Option<SpriteId>,
    half: Option<SpriteId>,
    half_blinking: Option<SpriteId>,
    absorbing_full_blinking: Option<SpriteId>,
    absorbing_half_blinking: Option<SpriteId>,
}

/// `PlayerTabOverlay.HealthState`: the hearts of a tab entry blink and lag behind a change.
#[derive(Debug, Clone, Copy)]
struct TabHealth {
    last: i32,
    displayed: i32,
    updated: i64,
    blink_until: i64,
}

impl TabHealth {
    fn new(value: i32) -> Self {
        Self {
            last: value,
            displayed: value,
            updated: 0,
            blink_until: 0,
        }
    }

    fn update(&mut self, value: i32, tick: i64) {
        if value != self.last {
            self.blink_until = tick + if value < self.last { 20 } else { 10 };
            self.last = value;
            self.updated = tick;
        }
        if tick - self.updated > 20 {
            self.displayed = value;
        }
    }

    fn blinking(&self, tick: i64) -> bool {
        self.blink_until > tick && (self.blink_until - tick) % 6 >= 3
    }
}

/// The selected item's name, shown above the hotbar after a slot change (`toolHighlightTimer`).
#[derive(Debug, Default)]
struct ToolHighlight {
    timer: i32,
    item: i32,
    hover: Option<Component>,
    styled: Option<Component>,
    cells: Option<(Vec<mcr_ui::Cell>, f32)>,
}

#[derive(Debug, Clone, Copy)]
struct EffectEntry {
    sprite: Option<SpriteId>,
    beneficial: bool,
    color: i32,
}

#[derive(Debug)]
struct SpectatorSprites {
    teleport_to_player: Option<SpriteId>,
    teleport_to_team: Option<SpriteId>,
    scroll_left: Option<SpriteId>,
    scroll_right: Option<SpriteId>,
    close: Option<SpriteId>,
}

/// The spectator hotbar menu as `SpectatorGui` draws it, fading out after the last key press.
pub struct SpectatorHud<'a> {
    pub menu: &'a SpectatorMenu,
    pub alpha: f32,
    pub keys: [InputKey; 9],
}

/// `Hud#tickAutosaveIndicator`: eases toward 1 while the local server saves, back to 0 after.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct AutosaveFade {
    value: f32,
    last: f32,
}

impl AutosaveFade {
    pub fn tick(&mut self, saving: bool) {
        self.last = self.value;
        self.value += (if saving { 1.0 } else { 0.0 } - self.value) * 0.2;
    }

    /// The indicator's opacity between ticks, 0 to 1.
    pub fn alpha(&self, partial: f32) -> f32 {
        (self.last + (self.value - self.last) * partial).clamp(0.0, 1.0)
    }
}

pub struct HudInput<'a> {
    pub state: &'a ClientState,
    pub options: &'a Options,
    pub tick: u32,
    pub partial: f32,
    pub hidden: bool,
    /// Java draws the sleep fade itself in mod mode.
    pub sleep_overlay: bool,
    pub tab_held: bool,
    pub chat_open: bool,
    /// The saving indicator's opacity.
    pub autosave: f32,
    pub debug: Option<&'a DebugOverlay>,
    pub skins: &'a SkinStore,
    pub camera: mcr_client::locator::Camera,
    pub eye_of: &'a dyn Fn(&mcr_client::Entity) -> [f64; 3],
    pub show_heads: bool,
    pub millis: u64,
    pub spectator: Option<SpectatorHud<'a>>,
    /// `None` hides the crosshair: third person, or a spectator looking at nothing to open.
    pub crosshair: Option<CrosshairIndicator>,
    /// Attack charge for the hotbar attack indicator.
    pub hotbar_attack: Option<f32>,
    /// Charge and cooldown of a jumpable mount.
    pub jump_bar: Option<(f32, bool)>,
    /// The open screen lists the effects itself (`Screen#showsActiveEffects`).
    pub screen_shows_effects: bool,
    pub screen_assets: &'a ScreenAssets,
    pub trim_materials: &'a [String],
}

pub struct Hud {
    sprites: HudSprites,
    own: Sprites,
    heads: RefCell<FxHashMap<String, (Option<SpriteId>, u64)>>,
    head_sprites: RefCell<mcr_ui::OwnedSprites>,
    waypoint_styles: RefCell<FxHashMap<String, WaypointStyle>>,
    style_reader: Option<crate::skins::AssetReader>,
    pub chat: SharedChat,
    max_health_attr: Option<i32>,
    armor_attr: Option<i32>,
    poison: Option<i32>,
    wither: Option<i32>,
    regeneration: Option<i32>,
    hunger: Option<i32>,
    last_health: i32,
    display_health: i32,
    last_health_millis: u64,
    health_blink_tick: i64,
    last_bubble_pop: i32,
    /// Volume and pitch of `ui.hud.bubble_pop`, played by the caller after the frame.
    pub bubble_pop: Option<(f32, f32)>,
    tool: ToolHighlight,
    effect_entries: FxHashMap<i32, EffectEntry>,
    effect_sorted: Vec<(EffectSortKey, HudEffect)>,
    effect_icons: Vec<HudEffect>,
    tab_health: RefCell<FxHashMap<uuid::Uuid, TabHealth>>,
}

impl std::fmt::Debug for Hud {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Hud").finish_non_exhaustive()
    }
}

/// Splits `cells` into lines no wider than `width`, honouring newlines.
fn wrapped(frame: &mut UiFrame<'_>, cells: &[mcr_ui::Cell], width: f32) -> Vec<Vec<mcr_ui::Cell>> {
    frame
        .ui()
        .wrap(cells, width)
        .into_iter()
        .map(|r| cells[r].iter().filter(|c| c.ch != '\n').cloned().collect())
        .collect()
}

/// Blocks a spectator's crosshair shows over (`BlockBehaviour#showAsInteractableInSpectatorMode`):
/// those with a menu, and portals.
pub fn spectator_block(name: &str, has_book: bool) -> bool {
    let name = name.strip_prefix("minecraft:").unwrap_or(name);
    match name {
        "lectern" => has_book,
        "anvil" | "chipped_anvil" | "damaged_anvil" | "cartography_table" | "enchanting_table"
        | "crafting_table" | "chest" | "trapped_chest" | "smithing_table" | "grindstone"
        | "loom" | "stonecutter" | "furnace" | "blast_furnace" | "smoker" | "barrel" | "beacon"
        | "brewing_stand" | "crafter" | "hopper" | "dispenser" | "dropper" | "nether_portal"
        | "end_portal" | "end_gateway" => true,
        n => n.ends_with("shulker_box"),
    }
}

/// Entities that are a `MenuProvider`: container minecarts and chest boats.
pub fn spectator_entity(name: &str) -> bool {
    let name = name.strip_prefix("minecraft:").unwrap_or(name);
    matches!(name, "chest_minecart" | "hopper_minecart")
        || name.ends_with("_chest_boat")
        || name.ends_with("_chest_raft")
}

/// Java's `Float.toString` for the plain values a heart count takes.
fn java_float(v: f32) -> String {
    if v.fract() == 0.0 && v.abs() < 1.0e7 {
        format!("{v:.1}")
    } else {
        format!("{v}")
    }
}

/// Vanilla `Hud#extractSleepOverlay`: dark blue, rising over 100 ticks and fading over 10 after.
pub fn sleep_overlay_color(timer: u32) -> Option<u32> {
    if timer == 0 {
        return None;
    }
    let t = timer as f32;
    let amount = if t > 100.0 {
        1.0 - (t - 100.0) / 10.0
    } else {
        t / 100.0
    };
    let alpha = (220.0 * amount.clamp(0.0, 1.0)) as u32;
    Some(alpha << 24 | 0x10_1020)
}

fn draw_sleep_overlay(frame: &mut UiFrame<'_>, timer: u32) {
    if let Some(color) = sleep_overlay_color(timer) {
        let (w, h) = frame.gui_size();
        frame.fill_rect(0, 0, w, h, color);
    }
}

/// `Options#getBackgroundColor(defaultOpacity)`, as the alpha of a black fill.
fn background_alpha(options: &Options, default: f32) -> u32 {
    let opacity = if options.background_for_chat_only {
        default
    } else {
        options.text_background_opacity as f32
    };
    (opacity.clamp(0.0, 1.0) * 255.0).floor() as u32
}

/// The backdrop of `GuiGraphicsExtractor#textWithBackdrop`, faded with the text.
fn text_backdrop(frame: &mut UiFrame<'_>, options: &Options, x: i32, y: i32, w: i32, argb: u32) {
    let alpha = background_alpha(options, 0.0) * (argb >> 24) / 255;
    frame.fill_rect(x - 2, y - 2, x + w + 2, y + 9 + 2, alpha << 24);
}

pub fn chat_options(options: &Options, focused: bool) -> ChatOptions {
    let text_opacity = (options.chat_opacity * 0.9 + 0.1) as f32;
    let height = if focused {
        options.chat_height_focused
    } else {
        options.chat_height_unfocused
    };
    let line_spacing = options.chat_line_spacing as f32;
    ChatOptions {
        width: (options.chat_width * 280.0 + 40.0).floor() as i32,
        visible_lines: ChatOptions::lines_for_height(
            (height * 160.0 + 20.0).floor() as i32,
            line_spacing,
        ),
        background_opacity: options.text_background_opacity as f32,
        text_opacity,
        focused,
        scale: options.chat_scale as f32,
        line_spacing,
        colors: options.chat_colors,
        auto_suggestions: options.auto_suggestions,
    }
}

/// `chatDelay` in milliseconds.
pub(crate) fn chat_delay_ms(options: &Options) -> u64 {
    (options.chat_delay.clamp(0.0, 6.0) * 1000.0) as u64
}

impl Hud {
    /// Re-resolves sprite ids after the UI atlas was rebuilt, keeping chat and timers.
    pub fn rebind(&mut self, ui: &Ui, data: &'static Version) {
        let fresh = Self::new(ui, data);
        self.sprites = fresh.sprites;
        self.own = fresh.own;
        self.effect_entries.clear();
        self.tool.cells = None;
        self.heads.get_mut().clear();
        self.head_sprites.get_mut().forget();
    }

    pub fn with_style_reader(mut self, read: crate::skins::AssetReader) -> Self {
        self.style_reader = Some(read);
        self
    }

    fn waypoint_style(&self, id: &str) -> WaypointStyle {
        if let Some(s) = self.waypoint_styles.borrow().get(id) {
            return s.clone();
        }
        let (ns, path) = id.split_once(':').unwrap_or(("minecraft", id));
        let valid = |t: &str| {
            !t.is_empty()
                && t.bytes().all(|b| {
                    b.is_ascii_lowercase()
                        || b.is_ascii_digit()
                        || matches!(b, b'_' | b'.' | b'-' | b'/')
                })
                && !t.contains("..")
        };
        let style = (valid(ns) && valid(path))
            .then(|| {
                self.style_reader.as_ref()?(&format!("assets/{ns}/waypoint_style/{path}.json"))
            })
            .flatten()
            .and_then(|b| WaypointStyle::parse(&b))
            .unwrap_or_else(WaypointStyle::fallback);
        let mut styles = self.waypoint_styles.borrow_mut();
        if styles.len() < 64 {
            styles.insert(id.to_owned(), style.clone());
        }
        style
    }

    fn draw_locator(&self, frame: &mut UiFrame<'_>, input: &HudInput<'_>) {
        let (gui_w, gui_h) = frame.gui_size();
        let top = gui_h - 24 - 5;
        let marks = mcr_client::locator::marks(
            input.state.gameplay.waypoints.tracked.values(),
            &input.camera,
            input.state.uuid,
            &input.state.entities,
            input.eye_of,
        );
        let middle = ((gui_w - 9) as f32 / 2.0).ceil() as i32;
        for m in marks {
            let style = self.waypoint_style(&m.style);
            let index = mcr_client::locator::sprite_index(
                style.sprites.len(),
                style.near,
                style.far,
                m.distance,
            );
            let name = format!(
                "hud/locator_bar_dot/{}",
                style.sprites[index.min(style.sprites.len() - 1)]
            );
            let x = middle + m.offset;
            if let Some(dot) = frame.ui().sprite_id(&name) {
                frame.sprite_tinted(dot, x, top - 2, 9, 9, m.color);
            }
            let (arrow, y) = match m.pitch {
                mcr_client::locator::Pitch::None => continue,
                mcr_client::locator::Pitch::Down => (self.own.arrow_down, top + 6),
                mcr_client::locator::Pitch::Up => (self.own.arrow_up, top - 6),
            };
            if let Some(a) = arrow {
                frame.sprite(a, x + 1, y, 7, 5);
            }
        }
    }

    pub fn new(ui: &Ui, data: &'static Version) -> Self {
        let id = |name: &str| ui.sprite_id(name);
        let attributes = data.registry("minecraft:attribute");
        let attribute = |name: &str| {
            attributes
                .and_then(|r| r.id_of(name))
                .and_then(|i| i32::try_from(i).ok())
        };
        let effects = data.registry("minecraft:mob_effect");
        let effect = |name: &str| {
            effects
                .and_then(|r| r.id_of(name))
                .and_then(|i| i32::try_from(i).ok())
        };
        Self {
            sprites: HudSprites::new(ui),
            own: Sprites {
                hotbar: id("hud/hotbar"),
                selection: id("hud/hotbar_selection"),
                tab_hearts: TabHeartSprites {
                    container: id("hud/heart/container"),
                    container_blinking: id("hud/heart/container_blinking"),
                    full: id("hud/heart/full"),
                    full_blinking: id("hud/heart/full_blinking"),
                    half: id("hud/heart/half"),
                    half_blinking: id("hud/heart/half_blinking"),
                    absorbing_full_blinking: id("hud/heart/absorbing_full_blinking"),
                    absorbing_half_blinking: id("hud/heart/absorbing_half_blinking"),
                },
                arrow_up: id("hud/locator_bar_arrow_up"),
                arrow_down: id("hud/locator_bar_arrow_down"),
                spectator: SpectatorSprites {
                    teleport_to_player: id("spectator/teleport_to_player"),
                    teleport_to_team: id("spectator/teleport_to_team"),
                    scroll_left: id("spectator/scroll_left"),
                    scroll_right: id("spectator/scroll_right"),
                    close: id("spectator/close"),
                },
                ping: [
                    id("icon/ping_unknown"),
                    id("icon/ping_1"),
                    id("icon/ping_2"),
                    id("icon/ping_3"),
                    id("icon/ping_4"),
                    id("icon/ping_5"),
                ],
            },
            heads: RefCell::default(),
            head_sprites: RefCell::default(),
            waypoint_styles: RefCell::default(),
            style_reader: None,
            chat: SharedChat::default(),
            max_health_attr: attribute("minecraft:max_health"),
            armor_attr: attribute("minecraft:armor"),
            poison: effect("minecraft:poison"),
            wither: effect("minecraft:wither"),
            regeneration: effect("minecraft:regeneration"),
            hunger: effect("minecraft:hunger"),
            last_health: 0,
            display_health: 0,
            last_health_millis: 0,
            health_blink_tick: 0,
            last_bubble_pop: 0,
            bubble_pop: None,
            tool: ToolHighlight::default(),
            effect_entries: FxHashMap::default(),
            effect_sorted: Vec::new(),
            effect_icons: Vec::new(),
            tab_health: RefCell::default(),
        }
    }

    pub fn add_chat(
        &mut self,
        ui: &mut Ui,
        message: &Component,
        tick: u32,
        options: &Options,
        tag: Option<mcr_ui::ChatLineTag>,
        signature: Option<[u8; 256]>,
    ) {
        self.chat.borrow_mut().add_tagged(
            ui,
            message,
            tick,
            &chat_options(options, false),
            tag,
            signature,
        );
    }

    /// A player or disguised message, held back by `chatDelay` like `ChatListener#handleMessage`.
    #[allow(clippy::too_many_arguments)]
    pub fn add_player_chat(
        &mut self,
        ui: &mut Ui,
        message: &Component,
        tick: u32,
        options: &Options,
        tag: Option<mcr_ui::ChatLineTag>,
        signature: Option<[u8; 256]>,
        now_ms: u64,
    ) {
        self.chat.borrow_mut().add_delayed(
            ui,
            message,
            tick,
            &chat_options(options, false),
            tag,
            signature,
            (now_ms, chat_delay_ms(options)),
        );
    }

    /// `ClientPacketListener#handleDeleteChat`: a still queued message is dropped instead.
    pub fn delete_chat(
        &mut self,
        ui: &mut Ui,
        signature: &[u8; 256],
        tick: u32,
        options: &Options,
    ) {
        let mut chat = self.chat.borrow_mut();
        if !chat.remove_queued(signature) {
            chat.delete(ui, signature, tick, &chat_options(options, false));
        }
    }

    pub fn tick_chat(&mut self, ui: &mut Ui, tick: u32, options: &Options, now_ms: u64) {
        let opts = chat_options(options, false);
        let mut chat = self.chat.borrow_mut();
        chat.process_deletions(ui, tick, &opts);
        chat.tick_queue(ui, tick, &opts, (now_ms, chat_delay_ms(options)));
    }

    /// Delayed messages the chat showed since the last call and whether its queue ran dry.
    pub fn take_released_chat(&mut self) -> (Vec<[u8; 256]>, bool) {
        self.chat.borrow_mut().take_released()
    }

    pub fn accept_next_chat(&mut self, ui: &mut Ui, tick: u32, options: &Options, now_ms: u64) {
        self.chat
            .borrow_mut()
            .accept_next(ui, tick, &chat_options(options, false), now_ms);
    }

    pub fn clear_chat(&mut self) {
        *self.chat.borrow_mut() = mcr_ui::ChatHud::default();
    }

    /// Vanilla `Hud#tick`: the selected item name's timer.
    pub fn tick(&mut self, state: &ClientState, options: &Options) {
        let stack = state.inventory.hotbar(state.player.held_slot);
        if stack.is_empty() {
            self.tool.timer = 0;
            self.tool.item = 0;
            self.tool.hover = None;
            return;
        }
        let item = ItemRef::new(stack.item, stack.count, &stack.components);
        let cx = crate::view::TooltipCx(state);
        let hover = mcr_ui::hover_name(&item, &cx);
        if self.tool.hover.is_none()
            || self.tool.item != stack.item
            || self.tool.hover.as_ref() != Some(&hover)
        {
            self.tool.timer = (40.0 * options.notification_display_time) as i32;
            self.tool.styled = Some(mcr_ui::styled_hover_name(&item, &cx));
            self.tool.cells = None;
        } else if self.tool.timer > 0 {
            self.tool.timer -= 1;
        }
        self.tool.item = stack.item;
        self.tool.hover = Some(hover);
    }

    pub fn draw(&mut self, frame: &mut UiFrame<'_>, icons: &mut ItemIcons, input: &HudInput<'_>) {
        icons.set_time(input.millis);
        let o = input.options;
        icons.set_glint(o.glint_speed as f32, o.glint_strength as f32);
        mcr_entity_model::glint::set_options(o.glint_speed as f32, o.glint_strength as f32);
        if !input.hidden {
            self.draw_gameplay(frame, icons, input);
        }
        if input.sleep_overlay {
            draw_sleep_overlay(frame, input.state.player.sleep_timer);
        }
        if !input.hidden {
            self.draw_messages(frame, input);
        }
        if let Some(debug) = input.debug {
            debug.draw(frame);
        }
        if !input.hidden {
            self.draw_autosave(frame, input);
        }
    }

    /// `Hud#extractSavingIndicator`.
    fn draw_autosave(&self, frame: &mut UiFrame<'_>, input: &HudInput<'_>) {
        if !input.options.show_autosave_indicator {
            return;
        }
        let alpha = (255.0 * input.autosave.clamp(0.0, 1.0)).floor() as u32;
        if alpha == 0 {
            return;
        }
        let text = frame
            .ui()
            .lang()
            .get("menu.savingLevel")
            .unwrap_or("Saving world")
            .to_owned();
        let width = frame.ui().text_width(&text).ceil() as i32;
        let (gui_w, gui_h) = frame.gui_size();
        let (x, y) = (gui_w - width - 5, gui_h - 9 - 5);
        let argb = alpha << 24 | 0xFF_FFFF;
        text_backdrop(frame, input.options, x, y, width, argb);
        frame.text(&text, x as f32, y as f32, argb, true);
    }

    fn draw_gameplay(
        &mut self,
        frame: &mut UiFrame<'_>,
        icons: &mut ItemIcons,
        input: &HudInput<'_>,
    ) {
        let state = input.state;
        if let Some(indicator) = input.crosshair {
            self.sprites.draw_crosshair(frame, indicator);
        }
        self.draw_bars(frame, icons, input);
        if !input.screen_shows_effects {
            self.draw_effects(frame, input);
        }
        self.draw_bosses(frame, state);
    }

    /// The HUD parts vanilla draws over the sleep fade.
    fn draw_messages(&mut self, frame: &mut UiFrame<'_>, input: &HudInput<'_>) {
        self.draw_sidebar(frame, input);
        self.draw_titles(frame, input);
        if !input.chat_open {
            let opts = chat_options(input.options, false);
            self.chat.borrow_mut().draw(frame, input.tick, &opts);
        }
        if input.tab_held {
            self.draw_tab_list(frame, input);
        } else {
            self.tab_health.get_mut().clear();
        }
    }

    fn attribute(&self, state: &ClientState, id: Option<i32>) -> Option<f64> {
        state.player.attributes.get(&id?).map(|a| a.value(None))
    }

    fn effect_entry(&mut self, id: i32, input: &HudInput<'_>) -> Option<EffectEntry> {
        if let Some(e) = self.effect_entries.get(&id) {
            return Some(*e);
        }
        let name = input
            .state
            .data
            .registry("minecraft:mob_effect")?
            .name_of(u32::try_from(id).ok()?)?;
        let info = mcr_ui::effect_info(name);
        let path = name.split_once(':').map_or(name, |p| p.1);
        let entry = EffectEntry {
            sprite: input.screen_assets.texture(&format!("mob_effect/{path}")),
            beneficial: info.is_some_and(|i| i.category == EffectCategory::Beneficial),
            color: info.map_or(0, |i| i.color),
        };
        if self.effect_entries.len() < 256 {
            self.effect_entries.insert(id, entry);
        }
        Some(entry)
    }

    fn draw_effects(&mut self, frame: &mut UiFrame<'_>, input: &HudInput<'_>) {
        let mut sorted = std::mem::take(&mut self.effect_sorted);
        sorted.clear();
        for (&id, e) in &input.state.player.effects {
            if sorted.len() >= MAX_HUD_EFFECTS {
                break;
            }
            if e.flags & EFFECT_FLAG_SHOW_ICON == 0 {
                continue;
            }
            let Some(entry) = self.effect_entry(id, input) else {
                continue;
            };
            let ambient = e.flags & EFFECT_FLAG_AMBIENT != 0;
            sorted.push((
                EffectSortKey {
                    ambient,
                    duration: e.duration_ticks,
                    color: entry.color,
                },
                HudEffect {
                    sprite: entry.sprite,
                    beneficial: entry.beneficial,
                    ambient,
                    duration: e.duration_ticks,
                },
            ));
        }
        sorted.sort_by(|a, b| mcr_ui::effect_key_order(b.0, a.0));
        self.effect_icons.clear();
        self.effect_icons.extend(sorted.iter().map(|e| e.1));
        self.effect_sorted = sorted;
        self.sprites.draw_effects(frame, &self.effect_icons);
    }

    /// Hearts and current health of a living mount, `Hud#getVehicleMaxHearts`.
    fn vehicle_health(&self, state: &ClientState) -> (i32, i32) {
        let Some(vehicle) = state.player.vehicle.and_then(|v| state.entities.get(v)) else {
            return (0, 0);
        };
        let Some(EntityDataValue::Float(health)) = vehicle.data.get(&HEALTH_DATA_INDEX) else {
            return (0, 0);
        };
        let max = self
            .max_health_attr
            .and_then(|id| vehicle.attributes.get(&id))
            .map_or(20.0, |a| a.value(None));
        let max = if max.is_finite() {
            max.clamp(0.0, 2048.0)
        } else {
            20.0
        };
        let hearts = ((max as f32 + 0.5) as i32 / 2).min(MAX_VEHICLE_HEARTS);
        let health = if health.is_finite() {
            health.ceil().clamp(0.0, 2048.0) as i32
        } else {
            0
        };
        (hearts, health)
    }

    fn draw_bars(&mut self, frame: &mut UiFrame<'_>, icons: &mut ItemIcons, input: &HudInput<'_>) {
        let state = input.state;
        let player = &state.player;
        let spectator = player.game_mode == GameType::Spectator;
        let survival = matches!(player.game_mode, GameType::Survival | GameType::Adventure);
        let left_handed = input.options.main_hand == mcr_settings::value::HumanoidArm::Left;
        let offhand_stack = state.inventory.player.slots.get(OFFHAND_SLOT);
        let offhand = offhand_stack
            .and_then(StackView::new)
            .map(|v| v.with_trim_materials(input.trim_materials));
        let (vehicle_hearts, vehicle_health) = self.vehicle_health(state);
        let contextual = ContextualBar::choose(
            !state.gameplay.waypoints.tracked.is_empty(),
            input.jump_bar,
            survival,
            player.xp_show_ticks > 0,
        );
        let mut hud = HudState {
            tick: input.tick,
            selected_slot: player.held_slot,
            offhand: offhand.is_some(),
            left_handed,
            hotbar_attack: input.hotbar_attack,
            hotbar: !spectator,
            player_health: survival,
            hardcore: state.world.hardcore,
            vehicle_hearts,
            vehicle_health,
            contextual,
            xp_progress: player.xp_progress.clamp(0.0, 1.0),
            xp_level: player.xp_level.max(0),
            experience: survival,
            ..HudState::default()
        };
        if survival {
            self.fill_player_health(&mut hud, input);
        }
        if spectator {
            self.draw_spectator_menu(frame, input);
        } else {
            self.sprites.draw_hotbar(frame, &hud);
            let cooldown = |stack: Option<&mcr_protocol::types::ItemStack>| {
                stack.map_or(0.0, |s| state.cooldown_percent(s, input.partial))
            };
            let stacks: [Option<StackView<'_>>; 9] = std::array::from_fn(|i| {
                StackView::new(state.inventory.hotbar(i as u8))
                    .map(|v| v.with_trim_materials(input.trim_materials))
            });
            let pop = |slot: usize| {
                let p = f32::from(state.inventory.pop[slot]);
                if p > 0.0 { p - input.partial } else { 0.0 }
            };
            let items = HudItems {
                hotbar: std::array::from_fn(|i| {
                    let c = cooldown(Some(state.inventory.hotbar(i as u8)));
                    stacks[i].as_ref().map(|s| s.view(c))
                }),
                offhand: offhand.as_ref().map(|s| s.view(cooldown(offhand_stack))),
                pop: std::array::from_fn(pop),
                offhand_pop: pop(9),
                left_handed,
            };
            self.sprites.draw_items(frame, icons, &items);
        }
        self.sprites.draw_status(frame, &hud);
        if survival {
            self.track_bubble_pop(hud.air);
        }
        if contextual == ContextualBar::Locator {
            self.draw_locator(frame, input);
        }
        if !spectator {
            self.draw_tool_name(frame, input, survival);
        }
    }

    fn fill_player_health(&mut self, hud: &mut HudState, input: &HudInput<'_>) {
        let state = input.state;
        let player = &state.player;
        let max_health = self
            .attribute(state, self.max_health_attr)
            .map_or(20, |v| v.clamp(1.0, 1024.0) as i32);
        let absorption = match player.data.get(&ABSORPTION_DATA_INDEX) {
            Some(EntityDataValue::Float(v)) if v.is_finite() => v.ceil().clamp(0.0, 1024.0) as i32,
            _ => 0,
        };
        let health = player.health.ceil().clamp(0.0, 2048.0) as i32;
        let tick = i64::from(input.tick);
        let blink = self.health_blink_tick > tick && (self.health_blink_tick - tick) / 3 % 2 == 1;
        if health < self.last_health && player.damage_cooldown > 0 {
            self.last_health_millis = input.millis;
            self.health_blink_tick = tick + 20;
        } else if health > self.last_health && player.damage_cooldown > 0 {
            self.last_health_millis = input.millis;
            self.health_blink_tick = tick + 10;
        }
        if input.millis.saturating_sub(self.last_health_millis) > 1000 {
            self.display_health = health;
            self.last_health_millis = input.millis;
        }
        self.last_health = health;
        let has = |id: Option<i32>| id.is_some_and(|id| player.effects.contains_key(&id));
        let frozen = matches!(
            player.data.get(&FROZEN_DATA_INDEX),
            Some(EntityDataValue::Int(t)) if *t >= FULLY_FROZEN_TICKS
        );
        hud.heart_kind = if has(self.poison) {
            HeartKind::Poisoned
        } else if has(self.wither) {
            HeartKind::Withered
        } else if frozen {
            HeartKind::Frozen
        } else {
            HeartKind::Normal
        };
        hud.regen_index = if has(self.regeneration) {
            let span = (max_health.max(self.display_health).max(health) + 5).max(1);
            (input.tick as i32).rem_euclid(span)
        } else {
            -1
        };
        hud.blink = blink;
        hud.displayed_health = self.display_health;
        hud.shake = health + absorption <= 4;
        hud.health = health;
        hud.max_health = max_health;
        hud.absorption = absorption;
        hud.armor = self
            .attribute(state, self.armor_attr)
            .map_or(0, |v| v.clamp(0.0, 40.0) as i32);
        hud.food = player.food.clamp(0, 20);
        hud.hunger_effect = has(self.hunger);
        hud.saturation_empty = player.saturation <= 0.0;
        hud.air = AirState {
            supply: match player.data.get(&AIR_DATA_INDEX) {
                Some(EntityDataValue::Int(air)) => (*air).clamp(0, MAX_AIR),
                _ => MAX_AIR,
            },
            max: MAX_AIR,
            underwater: player.physics.eye_in_water(),
        };
    }

    /// `Hud#playAirBubblePoppedSound`, once per bursting bubble.
    fn track_bubble_pop(&mut self, air: AirState) {
        if !air.visible() {
            return;
        }
        if !air.underwater {
            self.last_bubble_pop = 0;
        }
        if let Some(bubble) = air.popping_bubble()
            && self.last_bubble_pop != bubble
        {
            self.bubble_pop = Some(air.pop_sound());
            self.last_bubble_pop = bubble;
        }
    }

    /// `Hud#extractSelectedItemName`.
    fn draw_tool_name(&mut self, frame: &mut UiFrame<'_>, input: &HudInput<'_>, survival: bool) {
        if self.tool.timer <= 0 {
            return;
        }
        let Some(name) = &self.tool.styled else {
            return;
        };
        if self.tool.cells.is_none() {
            let cells = frame.ui().flatten(name);
            let width = frame.ui().width(&cells);
            self.tool.cells = Some((cells, width));
        }
        let Some((cells, width)) = &self.tool.cells else {
            return;
        };
        let (gui_w, gui_h) = frame.gui_size();
        let w = width.ceil() as i32;
        let x = (gui_w - w) / 2;
        let y = gui_h - 59 + if survival { 0 } else { 14 };
        let alpha = (self.tool.timer * 256 / 10).min(255) as u32;
        if alpha > 0 {
            let argb = alpha << 24 | 0xFF_FFFF;
            text_backdrop(frame, input.options, x, y, w, argb);
            frame.cells(cells, x as f32, y as f32, argb, true);
        }
    }

    fn draw_spectator_menu(&self, frame: &mut UiFrame<'_>, input: &HudInput<'_>) {
        let Some(hud) = &input.spectator else {
            return;
        };
        let alpha = hud.alpha.clamp(0.0, 1.0);
        if alpha <= 0.0 {
            return;
        }
        let tint = |b: f32| {
            let c = (b * 255.0) as u32;
            ((alpha * 255.0) as u32) << 24 | c << 16 | c << 8 | c
        };
        let white = tint(1.0);
        let (gui_w, gui_h) = frame.gui_size();
        let left = gui_w / 2 - 91;
        let y = (gui_h as f32 - 22.0 * alpha).floor() as i32;
        let menu = hud.menu;
        if let Some(s) = self.own.hotbar {
            frame.sprite_tinted(s, left, y, 182, 22, white);
        }
        let selected = menu.selected_slot();
        if let (true, Some(s)) = ((0..9).contains(&selected), self.own.selection) {
            frame.sprite_tinted(s, left - 1 + selected * 20, y - 1, 24, 23, white);
        }
        let sprites = &self.own.spectator;
        for slot in 0..9 {
            let item = menu.item(slot);
            if item == MenuItem::Empty {
                continue;
            }
            let (x, iy) = (gui_w / 2 - 90 + slot * 20 + 2, y + 3);
            let enabled = item.enabled();
            let color = tint(if enabled { 1.0 } else { 0.25 });
            let icon = match item {
                MenuItem::TeleportToPlayer { .. } => sprites.teleport_to_player,
                MenuItem::TeleportToTeam { .. } => sprites.teleport_to_team,
                MenuItem::ScrollLeft => sprites.scroll_left,
                MenuItem::ScrollRight { .. } => sprites.scroll_right,
                MenuItem::Close => sprites.close,
                MenuItem::Player(p) => {
                    self.draw_face(frame, input, p.uuid, x + 2, iy + 2, white);
                    None
                }
                MenuItem::Team(t) => {
                    if let Some(rgb) = t.color {
                        let scale = |shift: u32| {
                            ((((rgb >> shift) & 0xFF) as f32 * if enabled { 1.0 } else { 0.25 })
                                as u32)
                                << shift
                        };
                        frame.fill_rect(
                            x + 1,
                            iy + 1,
                            x + 15,
                            iy + 15,
                            0xFF00_0000 | scale(16) | scale(8) | scale(0),
                        );
                    }
                    self.draw_face(frame, input, t.icon, x + 2, iy + 2, color);
                    None
                }
                MenuItem::Empty => None,
            };
            if let Some(icon) = icon {
                frame.sprite_tinted(icon, x, iy, 16, 16, color);
            }
            if enabled {
                let name = hud.keys[slot as usize].name();
                let label = frame.ui().lang().get(name).map_or_else(
                    || name.rsplit('.').next().unwrap_or(name).to_uppercase(),
                    str::to_owned,
                );
                let w = frame.ui().text_width(&label);
                frame.text(&label, (x + 17) as f32 - w, (iy + 9) as f32, white, true);
            }
        }
        let action = frame.ui().flatten(&menu.action_text());
        let w = frame.ui().width(&action);
        let x = (gui_w as f32 - w) / 2.0;
        let ty = gui_h - 35;
        if !input.options.background_for_chat_only {
            let bg = (input.options.text_background_opacity.clamp(0.0, 1.0)
                * f64::from(alpha)
                * 255.0) as u32;
            frame.fill_rect(
                x as i32 - 2,
                ty - 2,
                (x + w) as i32 + 2,
                ty + 9 + 2,
                bg << 24,
            );
        }
        frame.cells(&action, x.floor(), ty as f32, white, true);
    }

    fn draw_face(
        &self,
        frame: &mut UiFrame<'_>,
        input: &HudInput<'_>,
        uuid: uuid::Uuid,
        x: i32,
        y: i32,
        argb: u32,
    ) {
        if let Some(info) = input.state.players.get(uuid)
            && let Some(head) = self.head_sprite(frame, input.skins, info)
        {
            frame.sprite_tinted(head, x, y, 12, 12, argb);
        }
    }

    fn draw_bosses(&self, frame: &mut UiFrame<'_>, state: &ClientState) {
        let (gui_w, gui_h) = frame.gui_size();
        let now = Instant::now();
        let x = gui_w / 2 - 91;
        let mut y = 12;
        for (_, bar) in boss_bars_ordered(&state.boss_bars) {
            let color = match bar.color {
                BossBarColor::Pink => "pink",
                BossBarColor::Blue => "blue",
                BossBarColor::Red => "red",
                BossBarColor::Green => "green",
                BossBarColor::Yellow => "yellow",
                BossBarColor::Purple => "purple",
                BossBarColor::White => "white",
            };
            let notches = match bar.overlay {
                BossBarOverlay::Progress => None,
                BossBarOverlay::Notched6 => Some(6),
                BossBarOverlay::Notched10 => Some(10),
                BossBarOverlay::Notched12 => Some(12),
                BossBarOverlay::Notched20 => Some(20),
            };
            let shown = bar.displayed_progress(now);
            let progress = if shown.is_finite() {
                (shown.clamp(0.0, 1.0) * 183.0) as i32
            } else {
                0
            };
            let mut ids = |kind: &str| {
                (
                    frame.ui().sprite_id(&format!("boss_bar/{color}_{kind}")),
                    notches.and_then(|n| {
                        frame
                            .ui()
                            .sprite_id(&format!("boss_bar/notched_{n}_{kind}"))
                    }),
                )
            };
            let (bg, bg_notch) = ids("background");
            let (fg, fg_notch) = ids("progress");
            for s in [bg, bg_notch].into_iter().flatten() {
                frame.sprite(s, x, y, 182, 5);
            }
            if progress > 0 {
                for s in [fg, fg_notch].into_iter().flatten() {
                    frame.sprite_region(s, (0, 0), (x, y, progress.min(182), 5));
                }
            }
            let cells = frame.ui().flatten(&bar.name);
            let width = frame.ui().width(&cells);
            frame.cells(
                &cells,
                (gui_w as f32 - width) / 2.0,
                (y - 9) as f32,
                0xFFFF_FFFF,
                true,
            );
            y += 19;
            if y >= gui_h / 3 {
                break;
            }
        }
    }

    fn draw_sidebar(&self, frame: &mut UiFrame<'_>, input: &HudInput<'_>) {
        let state = input.state;
        let board = &state.scoreboard;
        let Some(name) = board.sidebar_objective(&state.name) else {
            return;
        };
        let Some(objective) = board.objectives.get(name) else {
            return;
        };
        let title = frame.ui().flatten(&objective.display_name);
        let title_w = frame.ui().width(&title);
        let spacer = frame.ui().text_width(": ");
        let rows: Vec<(Vec<mcr_ui::Cell>, Vec<mcr_ui::Cell>, f32)> = board
            .sidebar_rows(name)
            .iter()
            .map(|row| {
                let name = frame.ui().flatten(&row.name);
                let score = frame.ui().flatten(&row.score);
                let score_w = frame.ui().width(&score);
                (name, score, score_w)
            })
            .collect();
        let widest = rows
            .iter()
            .map(|(name, _, score_w)| {
                frame.ui().width(name)
                    + if *score_w > 0.0 {
                        spacer + score_w
                    } else {
                        0.0
                    }
            })
            .fold(title_w, f32::max)
            .ceil() as i32;
        let (gui_w, gui_h) = frame.gui_size();
        let row_h = 9;
        let count = rows.len() as i32;
        let bottom = gui_h / 2 + count * row_h / 3;
        let left = gui_w - widest - 3;
        let right = gui_w - 3 + 2;
        let header_y = bottom - count * row_h;
        let header_bg = background_alpha(input.options, 0.4) << 24;
        let rows_bg = background_alpha(input.options, 0.3) << 24;
        frame.fill_rect(
            left - 2,
            header_y - row_h - 1,
            right,
            header_y - 1,
            header_bg,
        );
        frame.fill_rect(left - 2, header_y - 1, right, bottom, rows_bg);
        frame.cells(
            &title,
            (left + widest / 2) as f32 - title_w / 2.0,
            (header_y - row_h) as f32,
            0xFFFF_FFFF,
            false,
        );
        for (i, (name, score, score_w)) in rows.iter().enumerate() {
            let y = (bottom - (count - i as i32) * row_h) as f32;
            frame.cells(name, left as f32, y, 0xFFFF_FFFF, false);
            frame.cells(score, right as f32 - score_w, y, 0xFFFF_FFFF, false);
        }
    }

    fn draw_titles(&self, frame: &mut UiFrame<'_>, input: &HudInput<'_>) {
        let t = &input.state.titles;
        let (gui_w, gui_h) = frame.gui_size();
        // Text below alpha 4 counts as opaque, so a fading line stops at 4.
        if let (Some(alpha), Some(bar)) = (t.overlay_alpha(input.partial), &t.action_bar) {
            let argb = t.overlay_color(input.partial, alpha.max(4));
            let cells = frame.ui().flatten(bar);
            let w = frame.ui().width(&cells).ceil() as i32;
            let (x, y) = (gui_w / 2 - w / 2, gui_h - 68 - 4);
            text_backdrop(frame, input.options, x, y, w, argb);
            frame.cells(&cells, x as f32, y as f32, argb, true);
        }
        if let (Some(alpha), Some(title)) = (t.title_alpha(input.partial), &t.title) {
            let argb = u32::from(alpha.max(4)) << 24 | 0xFF_FFFF;
            let backdrop = background_alpha(input.options, 0.0) * u32::from(alpha) / 255;
            let (cx, cy) = (gui_w / 2, gui_h / 2);
            for (component, scale, dy) in [(Some(title), 4, -10), (t.subtitle.as_ref(), 2, 5)] {
                let Some(c) = component else {
                    continue;
                };
                let cells = frame.ui().flatten(c);
                let w = frame.ui().width(&cells).ceil() as i32;
                let x = -(w / 2);
                frame.fill_rect(
                    cx + (x - 2) * scale,
                    cy + (dy - 2) * scale,
                    cx + (x + w + 2) * scale,
                    cy + (dy + 9 + 2) * scale,
                    backdrop << 24,
                );
                frame.cells_scaled(
                    &cells,
                    (cx + x * scale) as f32,
                    (cy + dy * scale) as f32,
                    argb,
                    true,
                    scale as f32,
                );
            }
        }
    }

    /// A listed player's face for screens such as Social Interactions.
    pub fn face_sprite(
        &self,
        frame: &mut UiFrame<'_>,
        skins: &SkinStore,
        state: &ClientState,
        uuid: uuid::Uuid,
    ) -> Option<SpriteId> {
        let info = state.players.get(uuid)?;
        self.head_sprite(frame, skins, info)
    }

    fn head_sprite(
        &self,
        frame: &mut UiFrame<'_>,
        skins: &SkinStore,
        player: &mcr_client::PlayerInfo,
    ) -> Option<SpriteId> {
        let (key, pixels) = skins.head(player.uuid, player.show_hat)?;
        let mut heads = self.heads.borrow_mut();
        let mut owned = self.head_sprites.borrow_mut();
        let now = frame.number();
        if let Some((id, used)) = heads.get_mut(&key) {
            *used = now;
            return *id;
        }
        if heads.len() >= MAX_HEAD_SPRITES {
            heads.retain(|_, (id, used)| {
                let keep = *used >= now;
                if let (false, Some(id)) = (keep, id) {
                    owned.free(frame.ui().sprites_mut(), *id);
                }
                keep
            });
        }
        let id = if heads.len() < MAX_HEAD_SPRITES {
            owned.add(frame.ui().sprites_mut(), HEAD_SIZE, HEAD_SIZE, &pixels)
        } else {
            None
        };
        heads.insert(key, (id, now));
        id
    }

    /// `PlayerTabOverlay#extractTablistHearts`.
    fn draw_tab_hearts(
        &self,
        frame: &mut UiFrame<'_>,
        (left, right, y): (i32, i32, i32),
        player: uuid::Uuid,
        score: i32,
        tick: i64,
    ) {
        let health = {
            let mut states = self.tab_health.borrow_mut();
            if !states.contains_key(&player) && states.len() >= MAX_TAB_HEALTH {
                return;
            }
            let h = states
                .entry(player)
                .or_insert_with(|| TabHealth::new(score));
            h.update(score, tick);
            *h
        };
        let (score64, displayed) = (i64::from(score), i64::from(health.displayed));
        let full_hearts = (score64.max(displayed) + 1).div_euclid(2);
        let to_render = score64.max(displayed).max(20) / 2;
        let blink = health.blinking(tick);
        if full_hearts <= 0 {
            return;
        }
        let per_heart = ((right - left - 4) as f32 / to_render as f32)
            .min(9.0)
            .floor() as i32;
        if per_heart <= 3 {
            let pct = (score as f32 / 20.0).clamp(0.0, 1.0);
            let color =
                0xFF00_0000 | (((1.0 - pct) * 255.0) as u32) << 16 | ((pct * 255.0) as u32) << 8;
            let hearts = java_float(score as f32 / 2.0);
            let hp = Component::translatable(
                "multiplayer.player.list.hp",
                vec![Arg::Str(hearts.clone())],
            );
            let mut cells = frame.ui().flatten(&hp);
            let mut w = frame.ui().width(&cells).ceil() as i32;
            if right - w < left {
                cells = frame.ui().flatten(&Component::text(hearts));
                w = frame.ui().width(&cells).ceil() as i32;
            }
            frame.cells(
                &cells,
                ((right + left - w) / 2) as f32,
                y as f32,
                color,
                true,
            );
            return;
        }
        let h = &self.own.tab_hearts;
        let container = if blink {
            h.container_blinking
        } else {
            h.container
        };
        let (full_hearts, to_render) = (full_hearts as i32, to_render as i32);
        let sprite = |frame: &mut UiFrame<'_>, s: Option<SpriteId>, heart: i32| {
            if let Some(s) = s {
                frame.sprite(s, left + heart * per_heart, y, 9, 9);
            }
        };
        for heart in full_hearts..to_render {
            sprite(frame, container, heart);
        }
        for heart in 0..full_hearts {
            sprite(frame, container, heart);
            let n = heart * 2 + 1;
            if blink {
                if n < health.displayed {
                    sprite(frame, h.full_blinking, heart);
                }
                if n == health.displayed {
                    sprite(frame, h.half_blinking, heart);
                }
            }
            if n < score {
                let s = if heart >= 10 {
                    h.absorbing_full_blinking
                } else {
                    h.full
                };
                sprite(frame, s, heart);
            }
            if n == score {
                let s = if heart >= 10 {
                    h.absorbing_half_blinking
                } else {
                    h.half
                };
                sprite(frame, s, heart);
            }
        }
    }

    fn draw_tab_list(&self, frame: &mut UiFrame<'_>, input: &HudInput<'_>) {
        let state = input.state;
        let head_w = if input.show_heads { 9 } else { 0 };
        let board = &state.scoreboard;
        let players = state.players.tab_order(board);
        let objective = board
            .display_slots
            .get(&LIST_SLOT)
            .and_then(|name| Some((name.as_str(), board.objectives.get(name)?)));
        let hearts = objective.is_some_and(|(_, o)| o.render_type == ObjectiveRenderType::Hearts);
        let spacer = frame.ui().text_width(" ");
        let entries: Vec<(Vec<mcr_ui::Cell>, Vec<mcr_ui::Cell>, f32)> = players
            .iter()
            .map(|p| {
                let name = frame.ui().flatten(&tab_name(p, board));
                let score = objective
                    .filter(|_| !hearts)
                    .and_then(|(o, _)| board.format_value(&p.name, o, ScoreStyle::PlayerList))
                    .map(|c| frame.ui().flatten(&c))
                    .unwrap_or_default();
                let score_w = frame.ui().width(&score);
                (name, score, score_w)
            })
            .collect();
        let max_name_w = entries
            .iter()
            .map(|(name, _, _)| frame.ui().width(name))
            .fold(0.0f32, f32::max)
            .ceil() as i32;
        let max_score_w = entries
            .iter()
            .map(|(_, _, w)| if *w > 0.0 { spacer + w } else { 0.0 })
            .fold(0.0f32, f32::max)
            .ceil() as i32;
        {
            let mut health = self.tab_health.borrow_mut();
            if !health.is_empty() {
                health.retain(|id, _| players.iter().any(|p| p.uuid == *id));
            }
        }
        let (gui_w, _) = frame.gui_size();
        let slots = players.len();
        let mut rows = slots;
        let mut columns = 1usize;
        while rows > 20 {
            columns += 1;
            rows = slots.div_ceil(columns);
        }
        let width_for_score = match objective {
            Some(_) if hearts => 90,
            Some(_) => max_score_w,
            None => 0,
        };
        let cols = columns as i32;
        let slot_w = ((head_w + max_name_w + width_for_score + 13) * cols).min(gui_w - 50) / cols;
        let mut max_line_w = slot_w * cols + (cols - 1) * TAB_COLUMN_GAP;
        let left = gui_w / 2 - max_line_w / 2;
        let lines = |frame: &mut UiFrame<'_>, c: &Component| {
            if c.to_plain_string().is_empty() {
                return Vec::new();
            }
            let cells = frame.ui().flatten(c);
            wrapped(frame, &cells, (gui_w - 50) as f32)
        };
        let header = lines(frame, &state.tab_list.header);
        let footer = lines(frame, &state.tab_list.footer);
        for line in header.iter().chain(&footer) {
            max_line_w = max_line_w.max(frame.ui().width(line).ceil() as i32);
        }
        let (bg_left, bg_right) = (
            gui_w / 2 - max_line_w / 2 - 1,
            gui_w / 2 + max_line_w / 2 + 1,
        );
        let mut y = 10;
        if !header.is_empty() {
            frame.fill_rect(
                bg_left,
                y - 1,
                bg_right,
                y + header.len() as i32 * 9,
                TAB_BACKGROUND,
            );
            for line in &header {
                let w = frame.ui().width(line);
                frame.cells(line, (gui_w as f32 - w) / 2.0, y as f32, 0xFFFF_FFFF, true);
                y += 9;
            }
            y += 1;
        }
        frame.fill_rect(
            bg_left,
            y - 1,
            bg_right,
            y + rows as i32 * 9,
            TAB_BACKGROUND,
        );
        for (i, (name, score, score_w)) in entries.iter().enumerate() {
            let (col, row) = ((i / rows) as i32, (i % rows) as i32);
            let x = left + col * slot_w + col * TAB_COLUMN_GAP;
            let ry = y + row * 9;
            frame.fill_rect(x, ry, x + slot_w, ry + 8, TAB_SLOT);
            let spectator = players[i].game_mode == GameType::Spectator;
            let color = if spectator {
                SPECTATOR_NAME
            } else {
                0xFFFF_FFFF
            };
            if input.show_heads
                && let Some(head) = self.head_sprite(frame, input.skins, players[i])
            {
                frame.sprite(head, x, ry, HEAD_SIZE as i32, HEAD_SIZE as i32);
            }
            let name_x = x + head_w;
            frame.push_scissor(x, ry, x + slot_w - 11, ry + 9);
            frame.cells(name, name_x as f32, ry as f32, color, true);
            frame.pop_scissor();
            if let (Some((o, _)), false) = (objective, spectator) {
                let score_left = name_x + max_name_w + 1;
                let score_right = score_left + width_for_score;
                if score_right - score_left > 5 {
                    if hearts {
                        let value = board
                            .scores
                            .get(&(players[i].name.clone(), o.to_owned()))
                            .map_or(0, |s| s.value);
                        self.draw_tab_hearts(
                            frame,
                            (score_left, score_right, ry),
                            players[i].uuid,
                            value,
                            i64::from(input.tick),
                        );
                    } else {
                        frame.cells(
                            score,
                            score_right as f32 - score_w,
                            ry as f32,
                            0xFFFF_FFFF,
                            true,
                        );
                    }
                }
            }
            let icon = match players[i].latency {
                l if l < 0 => 0,
                l if l < 150 => 5,
                l if l < 300 => 4,
                l if l < 600 => 3,
                l if l < 1000 => 2,
                _ => 1,
            };
            if let Some(s) = self.own.ping[icon] {
                frame.sprite(s, x + slot_w - 11, ry, 10, 8);
            }
        }
        if !footer.is_empty() {
            y += rows as i32 * 9 + 1;
            frame.fill_rect(
                bg_left,
                y - 1,
                bg_right,
                y + footer.len() as i32 * 9,
                TAB_BACKGROUND,
            );
            for line in &footer {
                let w = frame.ui().width(line);
                frame.cells(line, (gui_w as f32 - w) / 2.0, y as f32, 0xFFFF_FFFF, true);
                y += 9;
            }
        }
    }
}

#[cfg(test)]
mod autosave_tests {
    use super::*;

    #[test]
    fn the_indicator_fades_in_while_saving_and_out_afterwards() {
        let mut fade = AutosaveFade::default();
        assert_eq!(fade.alpha(0.5), 0.0);
        for _ in 0..20 {
            fade.tick(true);
        }
        assert!(fade.alpha(1.0) > 0.98);
        let full = fade.alpha(1.0);
        fade.tick(false);
        assert!(fade.alpha(0.0) >= fade.alpha(1.0));
        assert!(
            (fade.alpha(0.0) - full).abs() < 1e-6,
            "partial 0 shows the previous tick"
        );
        for _ in 0..40 {
            fade.tick(false);
        }
        assert!(fade.alpha(1.0) < 0.01);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        WaypointStyle, java_float, sleep_overlay_color, spectator_block, spectator_entity,
    };

    #[test]
    fn spectator_crosshair_targets() {
        assert!(spectator_block("minecraft:chest", false));
        assert!(spectator_block("minecraft:red_shulker_box", false));
        assert!(spectator_block("minecraft:nether_portal", false));
        assert!(!spectator_block("minecraft:lectern", false));
        assert!(spectator_block("minecraft:lectern", true));
        assert!(!spectator_block("minecraft:ender_chest", false));
        assert!(!spectator_block("minecraft:stone", false));
        assert!(spectator_entity("minecraft:hopper_minecart"));
        assert!(spectator_entity("minecraft:bamboo_chest_raft"));
        assert!(spectator_entity("minecraft:oak_chest_boat"));
        assert!(!spectator_entity("minecraft:oak_boat"));
        assert!(!spectator_entity("minecraft:villager"));
    }

    #[test]
    fn heart_counts_print_like_java_floats() {
        assert_eq!(java_float(10.0), "10.0");
        assert_eq!(java_float(9.5), "9.5");
    }

    #[test]
    fn sleep_fade_rises_then_fades_after_waking() {
        assert_eq!(sleep_overlay_color(0), None);
        assert_eq!(sleep_overlay_color(50), Some(110 << 24 | 0x10_1020));
        assert_eq!(sleep_overlay_color(100), Some(220 << 24 | 0x10_1020));
        assert_eq!(sleep_overlay_color(105), Some(110 << 24 | 0x10_1020));
        assert_eq!(sleep_overlay_color(110), Some(0x10_1020));
    }

    #[test]
    fn style_json_is_bounded_and_validated() {
        let s = WaypointStyle::parse(
            br#"{"near_distance":64,"sprites":["minecraft:bowtie","minecraft:default_0"]}"#,
        )
        .unwrap();
        assert_eq!((s.near, s.far), (64.0, 332.0));
        assert_eq!(s.sprites, ["bowtie", "default_0"]);
        assert!(WaypointStyle::parse(br#"{"sprites":[]}"#).is_none());
        assert!(WaypointStyle::parse(br#"{"near_distance":400,"sprites":["a"]}"#).is_none());
        let many = format!(r#"{{"sprites":[{}]}}"#, vec![r#""a""#; 100].join(","));
        assert_eq!(
            WaypointStyle::parse(many.as_bytes()).unwrap().sprites.len(),
            16
        );
        assert!(WaypointStyle::parse(br#"{"sprites":["../x"]}"#).is_none());
    }
}
