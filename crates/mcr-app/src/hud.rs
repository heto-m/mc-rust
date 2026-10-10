use mcr_client::{BossBar, ClientState};
use mcr_data::Version;
use mcr_protocol::play::clientbound::ui::{BossBarColor, BossBarOverlay};
use mcr_protocol::types::entity_data::EntityDataValue;
use mcr_protocol::types::game::GameType;
use mcr_protocol::types::text::Component;
use mcr_settings::Options;
use mcr_ui::{
    ChatOptions, DebugOverlay, HudItems, HudSprites, HudState, ItemIcons, SharedChat, SpriteId,
    StackView, Ui, UiFrame,
};
use uuid::Uuid;

const MAX_AIR: i32 = 300;
const AIR_DATA_INDEX: u8 = 1;
const ABSORPTION_DATA_INDEX: u8 = 17;
const SIDEBAR_SLOT: u8 = 1;
const MAX_SIDEBAR_ROWS: usize = 15;
const MAX_TAB_ENTRIES: usize = 80;
const ACTIONBAR_TICKS: i32 = 60;
const OFFHAND_SLOT: usize = 45;
const OBJECTIVE_VALUE: u32 = 0xFFFF_5555;

#[derive(Debug)]
struct Sprites {
    crosshair: Option<SpriteId>,
    air: Option<SpriteId>,
    air_bursting: Option<SpriteId>,
    hotbar: Option<SpriteId>,
    selection: Option<SpriteId>,
    offhand: Option<SpriteId>,
    ping: [Option<SpriteId>; 6],
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
    pub millis: u64,
}

pub struct Hud {
    sprites: HudSprites,
    own: Sprites,
    pub chat: SharedChat,
    max_health_attr: Option<i32>,
    armor_attr: Option<i32>,
    title_ticks: i32,
    actionbar_ticks: i32,
}

impl std::fmt::Debug for Hud {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Hud").finish_non_exhaustive()
    }
}

const MAX_TITLE_PHASE: i32 = 72_000;

fn phase(v: i32) -> i32 {
    v.clamp(0, MAX_TITLE_PHASE)
}

fn title_total(state: &ClientState) -> i32 {
    let t = &state.titles;
    phase(t.fade_in) + phase(t.stay) + phase(t.fade_out)
}

/// Keeps the `limit` smallest entries under `cmp`, sorted.
fn smallest<T>(
    mut items: Vec<T>,
    limit: usize,
    cmp: impl Fn(&T, &T) -> std::cmp::Ordering,
) -> Vec<T> {
    if items.len() > limit {
        items.select_nth_unstable_by(limit, &cmp);
        items.truncate(limit);
    }
    items.sort_by(cmp);
    items
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
    pub fn new(ui: &Ui, data: &'static Version) -> Self {
        let id = |name: &str| ui.sprite_id(name);
        let attributes = data.registry("minecraft:attribute");
        let attribute = |name: &str| {
            attributes
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
                ping: [
                    id("icon/ping_unknown"),
                    id("icon/ping_1"),
                    id("icon/ping_2"),
                    id("icon/ping_3"),
                    id("icon/ping_4"),
                    id("icon/ping_5"),
                ],
            },
            chat: SharedChat::default(),
            max_health_attr: attribute("minecraft:max_health"),
            armor_attr: attribute("minecraft:armor"),
            title_ticks: 0,
            actionbar_ticks: 0,
        }
    }

    pub fn add_chat(&mut self, ui: &mut Ui, message: &Component, tick: u32, options: &Options) {
        self.chat
            .borrow_mut()
            .add(ui, message, tick, &chat_options(options, false));
    }

    pub fn show_title(&mut self, state: &ClientState) {
        self.title_ticks = title_total(state);
    }

    pub fn clear_titles(&mut self) {
        self.title_ticks = 0;
        self.actionbar_ticks = 0;
    }

    pub fn show_actionbar(&mut self) {
        self.actionbar_ticks = ACTIONBAR_TICKS;
    }

    pub fn tick(&mut self) {
        self.title_ticks = (self.title_ticks - 1).max(0);
        self.actionbar_ticks = (self.actionbar_ticks - 1).max(0);
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
            self.draw_tab_list(frame, state);
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
        let stacks: Vec<Option<StackView<'_>>> = (0..9u8)
            .map(|slot| StackView::new(state.inventory.hotbar(slot)))
            .collect();
        let offhand = state
            .inventory
            .player
            .slots
            .get(OFFHAND_SLOT)
            .and_then(StackView::new);
        let items = HudItems {
            hotbar: std::array::from_fn(|i| stacks[i].as_ref().map(|s| s.view(0.0))),
            offhand: offhand.as_ref().map(|s| s.view(0.0)),
        };
        let survival = matches!(player.game_mode, GameType::Survival | GameType::Adventure);
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
            let hud = HudState {
                selected_slot: player.held_slot,
                offhand: offhand.is_some(),
                health,
                displayed_health: health,
                max_health,
                absorption,
                hardcore: state.world.hardcore,
                armor: self
                    .attribute(state, self.armor_attr)
                    .map_or(0, |v| v.clamp(0.0, 40.0) as i32),
                food: player.food.clamp(0, 20),
                xp_progress: player.xp_progress.clamp(0.0, 1.0),
                xp_level: player.xp_level.max(0),
                ..HudState::default()
            };
            self.sprites.draw(frame, &hud);
            self.draw_air(frame, player.data.get(&AIR_DATA_INDEX));
        } else {
            self.draw_hotbar(frame, player.held_slot, offhand.is_some());
        }
        self.sprites.draw_items(frame, icons, &items);
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
        let (gui_w, _) = frame.gui_size();
        let mut bars: Vec<(&Uuid, &BossBar)> = state.boss_bars.iter().collect();
        bars.sort_by_key(|(id, _)| **id);
        let x = gui_w / 2 - 91;
        for (row, (_, bar)) in bars.into_iter().take(4).enumerate() {
            let y = 12 + row as i32 * 19;
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
            let progress = (bar.progress.clamp(0.0, 1.0) * 183.0) as i32;
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
        }
    }

    fn draw_sidebar(&self, frame: &mut UiFrame<'_>, state: &ClientState) {
        let board = &state.scoreboard;
        let Some(name) = board.display_slots.get(&SIDEBAR_SLOT) else {
            return;
        };
        let Some(objective) = board.objectives.get(name) else {
            return;
        };
        let rows: Vec<(&str, i32, Option<&Component>)> = board
            .scores
            .iter()
            .filter(|((owner, obj), _)| obj == name && !owner.starts_with('#'))
            .map(|((owner, _), s)| (owner.as_str(), s.value, s.display.as_ref()))
            .collect();
        let rows = smallest(rows, MAX_SIDEBAR_ROWS, |a, b| {
            b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0))
        });
        let title = frame.ui().flatten(&objective.display_name);
        let title_w = frame.ui().width(&title);
        let rendered: Vec<(Vec<mcr_ui::Cell>, f32, String, f32)> = rows
            .iter()
            .map(|(owner, value, display)| {
                let cells = match display {
                    Some(c) => frame.ui().flatten(c),
                    None => mcr_ui::parse_legacy(owner),
                };
                let w = frame.ui().width(&cells);
                let score = value.to_string();
                let sw = frame.ui().text_width(&score);
                (cells, w, score, sw)
            })
            .collect();
        let widest = rendered
            .iter()
            .map(|(_, w, _, sw)| w + 2.0 + sw)
            .fold(title_w, f32::max);
        let (gui_w, gui_h) = frame.gui_size();
        let row_h = 9;
        let total_h = rendered.len() as i32 * row_h;
        let bottom = gui_h / 2 + total_h / 3;
        let right = gui_w - 3;
        let left = right - widest.ceil() as i32 - 2;
        let top = bottom - total_h;
        frame.fill_rect(left - 2, top - row_h - 1, right + 2, top - 1, 0x6600_0000);
        frame.fill_rect(left - 2, top - 1, right + 2, bottom, 0x5000_0000);
        frame.cells(
            &title,
            left as f32 + (widest - title_w) / 2.0 + 1.0,
            (top - row_h) as f32,
            0xFFFF_FFFF,
            false,
        );
        for (i, (cells, _, score, sw)) in rendered.iter().enumerate() {
            let y = (top + i as i32 * row_h) as f32;
            frame.cells(cells, left as f32 + 1.0, y, 0xFFFF_FFFF, false);
            frame.text(score, right as f32 - sw - 1.0, y, OBJECTIVE_VALUE, false);
        }
    }

    fn draw_titles(&self, frame: &mut UiFrame<'_>, input: &HudInput<'_>) {
        let t = &input.state.titles;
        let (gui_w, gui_h) = frame.gui_size();
        if self.actionbar_ticks > 0
            && let Some(bar) = &t.action_bar
        {
            let left = self.actionbar_ticks as f32 - input.partial;
            let alpha = ((left * 255.0 / 20.0) as i32).clamp(0, 255) as u32;
            if alpha > 8 {
                let cells = frame.ui().flatten(bar);
                let w = frame.ui().width(&cells);
                frame.cells(
                    &cells,
                    (gui_w as f32 - w) / 2.0,
                    (gui_h - 68) as f32,
                    alpha << 24 | 0xFF_FFFF,
                    true,
                );
            }
        }
        if self.title_ticks > 0 {
            let (fade_in, stay, fade_out) = (phase(t.fade_in), phase(t.stay), phase(t.fade_out));
            let total = (fade_in + stay + fade_out).max(1);
            let age = total as f32 - (self.title_ticks as f32 - input.partial);
            let alpha = if age < fade_in as f32 {
                age / fade_in.max(1) as f32
            } else if age < (fade_in + stay) as f32 {
                1.0
            } else {
                1.0 - (age - (fade_in + stay) as f32) / fade_out.max(1) as f32
            };
            let alpha = (alpha.clamp(0.0, 1.0) * 255.0) as u32;
            if alpha > 8 {
                let argb = alpha << 24 | 0xFF_FFFF;
                let (cx, cy) = (gui_w as f32 / 2.0, gui_h as f32 / 2.0);
                for (component, scale, dy) in [(&t.title, 4.0, -10.0), (&t.subtitle, 2.0, 5.0)] {
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
    }

    fn draw_tab_list(&self, frame: &mut UiFrame<'_>, state: &ClientState) {
        let players: Vec<_> = state.players.iter().filter(|p| p.listed).collect();
        let players = smallest(players, MAX_TAB_ENTRIES, |a, b| {
            let spectator = |p: &&mcr_client::PlayerInfo| p.game_mode == GameType::Spectator;
            spectator(a)
                .cmp(&spectator(b))
                .then_with(|| b.list_order.cmp(&a.list_order))
                .then_with(|| a.name.cmp(&b.name))
        });
        let (gui_w, _) = frame.gui_size();
        let count = players.len().max(1);
        let mut columns = 1usize;
        let mut rows = count;
        while rows > 20 {
            columns += 1;
            rows = count.div_ceil(columns);
        }
        let cells: Vec<Vec<mcr_ui::Cell>> = players
            .iter()
            .map(|p| match &p.display_name {
                Some(c) => frame.ui().flatten(c),
                None => mcr_ui::parse_legacy(&p.name),
            })
            .collect();
        let name_w = cells
            .iter()
            .map(|c| frame.ui().width(c))
            .fold(0.0f32, f32::max)
            .ceil() as i32;
        let column_w = (name_w + 13).min((gui_w - 50) / columns as i32).max(40);
        let total_w = column_w * columns as i32 + (columns as i32 - 1);
        let left = (gui_w - total_w) / 2;
        let header = frame.ui().flatten(&state.tab_list.header);
        let footer = frame.ui().flatten(&state.tab_list.footer);
        let mut y = 10;
        if !header.is_empty() {
            let w = frame.ui().width(&header);
            frame.fill_rect(left - 1, y - 1, left + total_w + 1, y + 9, 0x8000_0000);
            frame.cells(
                &header,
                (gui_w as f32 - w) / 2.0,
                y as f32,
                0xFFFF_FFFF,
                true,
            );
            y += 10;
        }
        frame.fill_rect(
            left - 1,
            y - 1,
            left + total_w + 1,
            y + rows as i32 * 9 + 1,
            0x8000_0000,
        );
        for (i, cell) in cells.iter().enumerate() {
            let (col, row) = (i / rows, i % rows);
            let x = left + col as i32 * (column_w + 1);
            let ry = y + row as i32 * 9;
            frame.fill_rect(x, ry, x + column_w, ry + 8, 0x20FF_FFFF);
            frame.push_scissor(x, ry, x + column_w - 11, ry + 9);
            frame.cells(cell, x as f32 + 1.0, ry as f32, 0xFFFF_FFFF, true);
            frame.pop_scissor();
            let latency = players[i].latency;
            let icon = match latency {
                l if l < 0 => 0,
                l if l < 150 => 5,
                l if l < 300 => 4,
                l if l < 600 => 3,
                l if l < 1000 => 2,
                _ => 1,
            };
            if let Some(s) = self.own.ping[icon] {
                frame.sprite(s, x + column_w - 11, ry, 10, 8);
            }
        }
        y += rows as i32 * 9 + 2;
        if !footer.is_empty() {
            let w = frame.ui().width(&footer);
            frame.fill_rect(left - 1, y - 1, left + total_w + 1, y + 9, 0x8000_0000);
            frame.cells(
                &footer,
                (gui_w as f32 - w) / 2.0,
                y as f32,
                0xFFFF_FFFF,
                true,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_keeps_the_best_entries_sorted() {
        let items: Vec<i32> = (0..100_000).rev().collect();
        let top = smallest(items, 15, |a, b| a.cmp(b));
        assert_eq!(top, (0..15).collect::<Vec<_>>());
        assert_eq!(smallest(vec![3, 1, 2], 80, |a, b| a.cmp(b)), [1, 2, 3]);
    }

    #[test]
    fn hostile_title_times_do_not_overflow() {
        let mut state = ClientState::new(&mcr_client::ClientConfig::default(), Uuid::nil());
        state.titles.fade_in = i32::MAX;
        state.titles.stay = i32::MAX;
        state.titles.fade_out = i32::MIN;
        assert_eq!(title_total(&state), 2 * MAX_TITLE_PHASE);
    }
}
