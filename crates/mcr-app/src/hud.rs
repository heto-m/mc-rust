use std::cell::RefCell;
use std::time::Instant;

use mcr_client::{ClientState, ScoreStyle, boss_bars_ordered, tab_name};
use mcr_data::Version;
use mcr_protocol::play::clientbound::ui::{BossBarColor, BossBarOverlay, ObjectiveRenderType};
use mcr_protocol::types::entity_data::EntityDataValue;
use mcr_protocol::types::game::GameType;
use mcr_protocol::types::text::Component;
use mcr_settings::Options;
use mcr_ui::{
    ChatOptions, DebugOverlay, HeartKind, HudItems, HudSprites, HudState, ItemIcons, SharedChat,
    SpriteId, StackView, Ui, UiFrame,
};
use rustc_hash::FxHashMap;

use crate::skins::{HEAD_SIZE, SkinStore};

const MAX_HEAD_SPRITES: usize = 256;
const MAX_AIR: i32 = 300;
const AIR_DATA_INDEX: u8 = 1;
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
    crosshair: Option<SpriteId>,
    air: Option<SpriteId>,
    air_bursting: Option<SpriteId>,
    hotbar: Option<SpriteId>,
    selection: Option<SpriteId>,
    offhand: Option<SpriteId>,
    ping: [Option<SpriteId>; 6],
    locator_bg: Option<SpriteId>,
    arrow_up: Option<SpriteId>,
    arrow_down: Option<SpriteId>,
}

pub struct HudInput<'a> {
    pub state: &'a ClientState,
    pub options: &'a Options,
    pub tick: u32,
    pub partial: f32,
    pub hidden: bool,
    pub tab_held: bool,
    pub chat_open: bool,
    pub debug: Option<&'a DebugOverlay>,
    pub skins: &'a SkinStore,
    pub camera: mcr_client::locator::Camera,
    pub eye_of: &'a dyn Fn(&mcr_client::Entity) -> [f64; 3],
    pub show_heads: bool,
    pub millis: u64,
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
    last_health: i32,
    display_health: i32,
    last_health_millis: u64,
    health_blink_tick: i64,
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

pub fn chat_options(options: &Options, focused: bool) -> ChatOptions {
    let text_opacity = (options.chat_opacity * 0.9 + 0.1) as f32;
    let height = if focused {
        options.chat_height_focused
    } else {
        options.chat_height_unfocused
    };
    ChatOptions {
        width: (options.chat_width * 280.0 + 40.0) as i32,
        visible_lines: ((height * 160.0 + 20.0) / 9.0) as usize,
        background_opacity: options.text_background_opacity as f32,
        text_opacity,
        focused,
    }
}

impl Hud {
    /// Re-resolves sprite ids after the UI atlas was rebuilt, keeping chat and timers.
    pub fn rebind(&mut self, ui: &Ui, data: &'static Version) {
        let fresh = Self::new(ui, data);
        self.sprites = fresh.sprites;
        self.own = fresh.own;
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
        let (left, top) = ((gui_w - 182) / 2, gui_h - 24 - 5);
        if let Some(bg) = self.own.locator_bg {
            frame.sprite(bg, left, top, 182, 5);
        }
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
                crosshair: id("hud/crosshair"),
                air: id("hud/air"),
                air_bursting: id("hud/air_bursting"),
                hotbar: id("hud/hotbar"),
                selection: id("hud/hotbar_selection"),
                offhand: id("hud/hotbar_offhand_left"),
                locator_bg: id("hud/locator_bar_background"),
                arrow_up: id("hud/locator_bar_arrow_up"),
                arrow_down: id("hud/locator_bar_arrow_down"),
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
            last_health: 0,
            display_health: 0,
            last_health_millis: 0,
            health_blink_tick: 0,
        }
    }

    pub fn add_chat(&mut self, ui: &mut Ui, message: &Component, tick: u32, options: &Options) {
        self.chat
            .borrow_mut()
            .add(ui, message, tick, &chat_options(options, false));
    }

    pub fn clear_chat(&mut self) {
        *self.chat.borrow_mut() = mcr_ui::ChatHud::default();
    }

    pub fn draw(&mut self, frame: &mut UiFrame<'_>, icons: &mut ItemIcons, input: &HudInput<'_>) {
        icons.set_time(input.millis);
        if !input.hidden {
            self.draw_gameplay(frame, icons, input);
        }
        if let Some(debug) = input.debug {
            debug.draw(frame);
        }
    }

    fn draw_gameplay(
        &mut self,
        frame: &mut UiFrame<'_>,
        icons: &mut ItemIcons,
        input: &HudInput<'_>,
    ) {
        let state = input.state;
        let (gui_w, gui_h) = frame.gui_size();
        if let Some(c) = self.own.crosshair {
            frame.sprite(c, (gui_w - 15) / 2, (gui_h - 15) / 2, 15, 15);
        }
        self.draw_bars(frame, icons, input);
        self.draw_bosses(frame, state);
        self.draw_sidebar(frame, state);
        self.draw_titles(frame, input);
        if input.tab_held {
            self.draw_tab_list(frame, input);
        }
        if !input.chat_open {
            let opts = chat_options(input.options, false);
            self.chat.borrow_mut().draw(frame, input.tick, &opts);
        }
    }

    fn attribute(&self, state: &ClientState, id: Option<i32>) -> Option<f64> {
        state.player.attributes.get(&id?).map(|a| a.value(None))
    }

    fn draw_bars(&mut self, frame: &mut UiFrame<'_>, icons: &mut ItemIcons, input: &HudInput<'_>) {
        let state = input.state;
        let player = &state.player;
        let cooldown = |stack: Option<&mcr_protocol::types::ItemStack>| {
            stack.map_or(0.0, |s| state.cooldown_percent(s, input.partial))
        };
        let stacks: Vec<Option<StackView<'_>>> = (0..9u8)
            .map(|slot| StackView::new(state.inventory.hotbar(slot)))
            .collect();
        let offhand_stack = state.inventory.player.slots.get(OFFHAND_SLOT);
        let offhand = offhand_stack.and_then(StackView::new);
        let items = HudItems {
            hotbar: std::array::from_fn(|i| {
                let c = cooldown(Some(state.inventory.hotbar(i as u8)));
                stacks[i].as_ref().map(|s| s.view(c))
            }),
            offhand: offhand.as_ref().map(|s| s.view(cooldown(offhand_stack))),
        };
        let survival = matches!(player.game_mode, GameType::Survival | GameType::Adventure);
        let locator =
            !state.gameplay.waypoints.tracked.is_empty() && !(survival && player.xp_show_ticks > 0);
        if survival {
            let max_health = self
                .attribute(state, self.max_health_attr)
                .map_or(20, |v| v.clamp(1.0, 1024.0) as i32);
            let absorption = match player.data.get(&ABSORPTION_DATA_INDEX) {
                Some(EntityDataValue::Float(v)) if v.is_finite() => {
                    v.ceil().clamp(0.0, 1024.0) as i32
                }
                _ => 0,
            };
            let health = player.health.ceil().clamp(0.0, 2048.0) as i32;
            let tick = i64::from(input.tick);
            let blink =
                self.health_blink_tick > tick && (self.health_blink_tick - tick) / 3 % 2 == 1;
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
            let heart_kind = if has(self.poison) {
                HeartKind::Poisoned
            } else if has(self.wither) {
                HeartKind::Withered
            } else if frozen {
                HeartKind::Frozen
            } else {
                HeartKind::Normal
            };
            let regen_index = if has(self.regeneration) {
                let span = (max_health.max(self.display_health).max(health) + 5).max(1);
                (input.tick as i32).rem_euclid(span)
            } else {
                -1
            };
            let hud = HudState {
                blink,
                displayed_health: self.display_health,
                heart_kind,
                regen_index,
                shake_tick: (health + absorption <= 4).then_some(input.tick),
                selected_slot: player.held_slot,
                offhand: offhand.is_some(),
                health,
                max_health,
                absorption,
                hardcore: state.world.hardcore,
                armor: self
                    .attribute(state, self.armor_attr)
                    .map_or(0, |v| v.clamp(0.0, 40.0) as i32),
                food: player.food.clamp(0, 20),
                xp_progress: player.xp_progress.clamp(0.0, 1.0),
                xp_level: player.xp_level.max(0),
                xp_bar: !locator,
                ..HudState::default()
            };
            self.sprites.draw(frame, &hud);
            self.draw_air(frame, player.data.get(&AIR_DATA_INDEX));
        } else {
            self.draw_hotbar(frame, player.held_slot, offhand.is_some());
        }
        self.sprites.draw_items(frame, icons, &items);
        if locator {
            self.draw_locator(frame, input);
        }
    }

    fn draw_hotbar(&self, frame: &mut UiFrame<'_>, selected: u8, offhand: bool) {
        let (gui_w, gui_h) = frame.gui_size();
        let left = gui_w / 2 - 91;
        if let Some(s) = self.own.hotbar {
            frame.sprite(s, left, gui_h - 22, 182, 22);
        }
        if let Some(s) = self.own.selection {
            frame.sprite(s, left - 1 + i32::from(selected) * 20, gui_h - 23, 24, 23);
        }
        if let (true, Some(s)) = (offhand, self.own.offhand) {
            frame.sprite(s, left - 29, gui_h - 23, 29, 24);
        }
    }

    fn draw_air(&self, frame: &mut UiFrame<'_>, air: Option<&EntityDataValue>) {
        let Some(EntityDataValue::Int(air)) = air else {
            return;
        };
        let air = (*air).clamp(-20, MAX_AIR);
        if air >= MAX_AIR {
            return;
        }
        let (gui_w, gui_h) = frame.gui_size();
        let y = gui_h - 49;
        let right = gui_w / 2 + 91;
        let full = ((air.max(0) - 2) * 10 + MAX_AIR - 1) / MAX_AIR;
        let total = (air.max(0) * 10 + MAX_AIR - 1) / MAX_AIR;
        for i in 0..total.max(0) {
            let sprite = if i < full {
                self.own.air
            } else {
                self.own.air_bursting
            };
            if let Some(s) = sprite {
                frame.sprite(s, right - i * 8 - 9, y, 9, 9);
            }
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

    fn draw_sidebar(&self, frame: &mut UiFrame<'_>, state: &ClientState) {
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
        frame.fill_rect(
            left - 2,
            header_y - row_h - 1,
            right,
            header_y - 1,
            0x6600_0000,
        );
        frame.fill_rect(left - 2, header_y - 1, right, bottom, 0x5000_0000);
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
        if let (Some(alpha), Some(bar)) = (t.overlay_alpha(input.partial), &t.action_bar)
            && alpha > 8
        {
            let cells = frame.ui().flatten(bar);
            let w = frame.ui().width(&cells);
            frame.cells(
                &cells,
                (gui_w as f32 - w) / 2.0,
                (gui_h - 68 - 4) as f32,
                u32::from(alpha) << 24 | 0xFF_FFFF,
                true,
            );
        }
        if let (Some(alpha), Some(title)) = (t.title_alpha(input.partial), &t.title)
            && alpha > 8
        {
            let argb = u32::from(alpha) << 24 | 0xFF_FFFF;
            let (cx, cy) = (gui_w as f32 / 2.0, gui_h as f32 / 2.0);
            for (component, scale, dy) in
                [(Some(title), 4.0, -10.0), (t.subtitle.as_ref(), 2.0, 5.0)]
            {
                if let Some(c) = component {
                    let cells = frame.ui().flatten(c);
                    let w = frame.ui().width(&cells);
                    frame.cells_scaled(
                        &cells,
                        cx - w * scale / 2.0,
                        cy + dy * scale,
                        argb,
                        true,
                        scale,
                    );
                }
            }
        }
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
            if objective.is_some() && !spectator && !hearts {
                let score_left = name_x + max_name_w + 1;
                let score_right = score_left + width_for_score;
                if score_right - score_left > 5 {
                    frame.cells(
                        score,
                        score_right as f32 - score_w,
                        ry as f32,
                        0xFFFF_FFFF,
                        true,
                    );
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
mod tests {
    use super::WaypointStyle;

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
