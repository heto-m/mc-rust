use std::ops::Range;

use mcr_protocol::play::clientbound::ui::{AdvancementFrame, PositionedAdvancement};
use mcr_protocol::types::item::ItemStack;
use mcr_protocol::types::text::Component;

use super::env::{Env, GameView, RenderCx};
use super::input::{Key, MouseButton, UiInput};
use super::menus::draw_menu_background;
use super::screen::{Screen, ScreenAction};
use super::widgets::{Button, WidgetEvent, WidgetEventKind, WidgetSet};
use crate::style::Cell;

const WINDOW_W: i32 = 252;
const WINDOW_H: i32 = 140;
const INSIDE_X: i32 = 9;
const INSIDE_Y: i32 = 18;
const INSIDE_W: i32 = 234;
const INSIDE_H: i32 = 113;
const SCROLL_SPEED: f32 = 16.0;
const MAX_PARENT_DEPTH: usize = 64;
const MAX_NODES_PER_TAB: usize = 1024;
const MAX_COORD: f32 = 4096.0;
const MAX_TEXT_CELLS: usize = 1024;
const MAX_HOVER_LINES: usize = 32;
const DONE: u32 = 1;
const WHITE: u32 = 0xFFFF_FFFF;
const SPLIT_OFFSETS: [i32; 5] = [0, 10, -10, 25, -25];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TabSide {
    Above,
    Below,
    Left,
    Right,
}

impl TabSide {
    fn of(index: usize) -> Option<(Self, usize, usize)> {
        let mut rest = index;
        for (side, max) in [
            (Self::Above, 8),
            (Self::Below, 8),
            (Self::Left, 5),
            (Self::Right, 5),
        ] {
            if rest < max {
                return Some((side, rest, max));
            }
            rest -= max;
        }
        None
    }

    fn size(self) -> (i32, i32) {
        match self {
            Self::Above | Self::Below => (28, 32),
            Self::Left | Self::Right => (32, 28),
        }
    }

    fn position(self, index: usize) -> (i32, i32) {
        let (w, h) = self.size();
        let i = index as i32;
        match self {
            Self::Above => ((w + 4) * i, -h + 4),
            Self::Below => ((w + 4) * i, 136),
            Self::Left => (-w + 4, h * i),
            Self::Right => (248, h * i),
        }
    }

    fn sprite(self, index: usize, max: usize, selected: bool) -> String {
        let (side, names) = match self {
            Self::Above => ("above", ["left", "middle", "right"]),
            Self::Below => ("below", ["left", "middle", "right"]),
            Self::Left => ("left", ["top", "middle", "bottom"]),
            Self::Right => ("right", ["top", "middle", "bottom"]),
        };
        let part = if index == 0 {
            names[0]
        } else if index == max - 1 {
            names[2]
        } else {
            names[1]
        };
        let suffix = if selected { "_selected" } else { "" };
        format!("advancements/tab_{side}_{part}{suffix}")
    }

    fn icon_offset(self) -> (i32, i32) {
        match self {
            Self::Above => (6, 9),
            Self::Below => (6, 6),
            Self::Left => (10, 5),
            Self::Right => (6, 5),
        }
    }
}

#[derive(Clone, Debug)]
struct HoverLayout {
    title_cells: Vec<Cell>,
    title_lines: Vec<Range<usize>>,
    description_cells: Vec<Cell>,
    description_lines: Vec<Range<usize>>,
    width: i32,
}

#[derive(Clone, Debug)]
struct Node {
    parent: Option<usize>,
    x: i32,
    y: i32,
    title: Component,
    description: Component,
    icon: ItemStack,
    frame: AdvancementFrame,
    hidden: bool,
    total: usize,
    completed: usize,
    has_progress: bool,
    hover: Option<HoverLayout>,
}

impl Node {
    fn done(&self) -> bool {
        self.total > 0 && self.completed == self.total
    }

    fn percent(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            self.completed as f32 / self.total as f32
        }
    }

    fn visible(&self) -> bool {
        !self.hidden || self.done()
    }

    fn type_color(&self) -> u32 {
        match self.frame {
            AdvancementFrame::Challenge => 0xFFAA_00AA,
            AdvancementFrame::Task | AdvancementFrame::Goal => 0xFF55_FF55,
        }
    }

    fn frame_sprite(&self, obtained: bool) -> &'static str {
        match (self.frame, obtained) {
            (AdvancementFrame::Task, true) => "advancements/task_frame_obtained",
            (AdvancementFrame::Task, false) => "advancements/task_frame_unobtained",
            (AdvancementFrame::Challenge, true) => "advancements/challenge_frame_obtained",
            (AdvancementFrame::Challenge, false) => "advancements/challenge_frame_unobtained",
            (AdvancementFrame::Goal, true) => "advancements/goal_frame_obtained",
            (AdvancementFrame::Goal, false) => "advancements/goal_frame_unobtained",
        }
    }
}

#[derive(Clone, Debug)]
struct Tab {
    id: String,
    title: Component,
    icon: ItemStack,
    background: Option<String>,
    nodes: Vec<Node>,
    scroll: (f32, f32),
    min: (i32, i32),
    max: (i32, i32),
    centered: bool,
    fade: f32,
    hovered: Option<usize>,
}

impl Tab {
    fn can_scroll_x(&self) -> bool {
        i64::from(self.max.0) - i64::from(self.min.0) > i64::from(INSIDE_W)
    }

    fn can_scroll_y(&self) -> bool {
        i64::from(self.max.1) - i64::from(self.min.1) > i64::from(INSIDE_H)
    }

    fn scroll_by(&mut self, dx: f32, dy: f32) {
        if self.can_scroll_x() {
            let low = -(self.max.0.saturating_sub(INSIDE_W) as f32);
            self.scroll.0 = mth_clamp(self.scroll.0 + dx, low, 0.0);
        }
        if self.can_scroll_y() {
            let low = -(self.max.1.saturating_sub(INSIDE_H) as f32);
            self.scroll.1 = mth_clamp(self.scroll.1 + dy, low, 0.0);
        }
    }

    fn offset(&self) -> (i32, i32) {
        (self.scroll.0.floor() as i32, self.scroll.1.floor() as i32)
    }

    fn center(&mut self) {
        if !self.centered {
            self.scroll = (
                (117 - (i64::from(self.max.0) + i64::from(self.min.0)) / 2) as f32,
                (56 - (i64::from(self.max.1) + i64::from(self.min.1)) / 2) as f32,
            );
            self.centered = true;
        }
    }

    fn node_at(&self, mx: i32, my: i32) -> Option<usize> {
        if mx <= 0 || mx >= INSIDE_W || my <= 0 || my >= INSIDE_H {
            return None;
        }
        let (ox, oy) = self.offset();
        self.nodes.iter().position(|n| {
            n.visible()
                && mx >= ox + n.x
                && mx <= ox + n.x + 26
                && my >= oy + n.y
                && my <= oy + n.y + 26
        })
    }
}

#[derive(Debug)]
pub struct AdvancementsScreen {
    gui: (i32, i32),
    widgets: WidgetSet,
    tabs: Vec<Tab>,
    selected: Option<usize>,
    revision: Option<u64>,
    server_tab: Option<String>,
    pending: Vec<ScreenAction>,
    mouse: (f32, f32),
    dragging: bool,
}

impl Default for AdvancementsScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl AdvancementsScreen {
    pub fn new() -> Self {
        Self {
            gui: (0, 0),
            widgets: WidgetSet::default(),
            tabs: Vec::new(),
            selected: None,
            revision: None,
            server_tab: None,
            pending: Vec::new(),
            mouse: (0.0, 0.0),
            dragging: false,
        }
    }

    pub fn selected_id(&self) -> Option<&str> {
        self.selected
            .and_then(|i| self.tabs.get(i))
            .map(|t| t.id.as_str())
    }

    fn window(&self) -> (i32, i32) {
        ((self.gui.0 - WINDOW_W) / 2, (self.gui.1 - WINDOW_H) / 2)
    }

    fn select(&mut self, index: usize) {
        if self.selected != Some(index) {
            self.selected = Some(index);
        }
        if let Some(tab) = self.tabs.get(index) {
            self.pending
                .push(ScreenAction::SeenAdvancements(Some(tab.id.clone())));
        }
    }

    fn sync(&mut self, view: &dyn GameView) {
        let revision = view.advancements_revision();
        let server_tab = view.advancement_selected_tab().map(str::to_owned);
        let changed = self.revision != Some(revision);
        if changed {
            self.revision = Some(revision);
            self.rebuild(view);
        }
        if server_tab != self.server_tab {
            self.server_tab.clone_from(&server_tab);
            if let Some(id) = server_tab
                && let Some(index) = self.tabs.iter().position(|t| t.id == id)
            {
                self.selected = Some(index);
            }
        }
        if changed && self.selected.is_none() && !self.tabs.is_empty() {
            self.select(0);
        }
    }

    fn rebuild(&mut self, view: &dyn GameView) {
        let old: Vec<(String, (f32, f32), bool)> = self
            .tabs
            .iter()
            .map(|t| (t.id.clone(), t.scroll, t.centered))
            .collect();
        let previous = self.selected_id().map(str::to_owned);
        self.tabs = build_tabs(view);
        for tab in &mut self.tabs {
            if let Some((_, scroll, centered)) = old.iter().find(|o| o.0 == tab.id) {
                tab.scroll = *scroll;
                tab.centered = *centered;
            }
        }
        self.selected = previous.and_then(|id| self.tabs.iter().position(|t| t.id == id));
    }

    fn close(&mut self, out: &mut Vec<ScreenAction>) {
        out.push(ScreenAction::SeenAdvancements(None));
        out.push(ScreenAction::Pop);
    }

    fn hover_layout(cx: &mut RenderCx<'_, '_>, node: &Node) -> HoverLayout {
        let mut lang_cells = cx.frame.ui().flatten(&node.title);
        lang_cells.truncate(MAX_TEXT_CELLS);
        let mut title_lines = cx.frame.ui().wrap(&lang_cells, 163.0);
        title_lines.truncate(MAX_HOVER_LINES);
        let title_width = title_lines
            .iter()
            .map(|r| cx.frame.ui().width(&lang_cells[r.clone()]) as i32)
            .max()
            .unwrap_or(0)
            .max(80);
        let progress_width = if node.total > 1 {
            let n = node.total.to_string();
            let text = cx.lang_fmt("advancements.progress", &[&n, &n]);
            cx.text_width(&text) + 8
        } else {
            0
        };
        let mut longest = 29 + title_width + progress_width;
        let mut description_cells = cx.frame.ui().flatten(&node.description);
        description_cells.truncate(MAX_TEXT_CELLS);
        let mut description_lines = optimal_lines(cx, &description_cells, longest);
        description_lines.truncate(MAX_HOVER_LINES);
        for r in &description_lines {
            longest = longest.max(cx.frame.ui().width(&description_cells[r.clone()]) as i32);
        }
        HoverLayout {
            title_cells: lang_cells,
            title_lines,
            description_cells,
            description_lines,
            width: longest + 3 + 5,
        }
    }

    fn render_inside(&mut self, cx: &mut RenderCx<'_, '_>, left: i32, top: i32) {
        let Some(tab) = self.selected.and_then(|i| self.tabs.get_mut(i)) else {
            cx.frame.fill_rect(
                left + INSIDE_X,
                top + INSIDE_Y,
                left + INSIDE_X + INSIDE_W,
                top + INSIDE_Y + INSIDE_H,
                0xFF00_0000,
            );
            let mid = left + INSIDE_X + 117;
            let empty = cx.lang("advancements.empty");
            let sad = cx.lang("advancements.sad_label");
            cx.centered_text(&empty, mid, top + INSIDE_Y + 56 - 4, WHITE, true);
            cx.centered_text(&sad, mid, top + INSIDE_Y + INSIDE_H - 9, WHITE, true);
            return;
        };
        tab.center();
        let (wl, wt) = (left + INSIDE_X, top + INSIDE_Y);
        cx.frame.push_scissor(wl, wt, wl + INSIDE_W, wt + INSIDE_H);
        let (ox, oy) = tab.offset();
        let tile = tab
            .background
            .as_deref()
            .and_then(|name| cx.assets.texture(name));
        match tile {
            Some(id) => {
                let left_edge = ox % 16;
                let top_edge = oy % 16;
                for x in -1..=15 {
                    for y in -1..=8 {
                        cx.frame.sprite(
                            id,
                            wl + left_edge + 16 * x,
                            wt + top_edge + 16 * y,
                            16,
                            16,
                        );
                    }
                }
            }
            None => cx
                .frame
                .fill_rect(wl, wt, wl + INSIDE_W, wt + INSIDE_H, 0xFF00_0000),
        }
        for background in [true, false] {
            for node in &tab.nodes {
                let Some(parent) = node.parent.and_then(|p| tab.nodes.get(p)) else {
                    continue;
                };
                connect(cx, (wl + ox, wt + oy), parent, node, background);
            }
        }
        for node in &tab.nodes {
            if !node.visible() {
                continue;
            }
            let frame = node.frame_sprite(node.percent() >= 1.0);
            cx.sprite(frame, wl + ox + node.x + 3, wt + oy + node.y, 26, 26);
            cx.stack(&node.icon, wl + ox + node.x + 8, wt + oy + node.y + 5);
        }
        cx.frame.pop_scissor();
    }

    fn render_window(&mut self, cx: &mut RenderCx<'_, '_>, left: i32, top: i32) {
        cx.texture(
            "advancements/window",
            (0, 0),
            (left, top, WINDOW_W, WINDOW_H),
        );
        let selected = self.selected;
        if self.tabs.len() > 1 {
            for i in 0..self.tabs.len() {
                let Some((side, local, max)) = TabSide::of(i) else {
                    continue;
                };
                let (x, y) = side.position(local);
                let (w, h) = side.size();
                let sprite = side.sprite(local, max, selected == Some(i));
                cx.sprite(&sprite, left + x, top + y, w, h);
            }
            for (i, tab) in self.tabs.iter().enumerate() {
                let Some((side, local, _)) = TabSide::of(i) else {
                    continue;
                };
                let (x, y) = side.position(local);
                let (dx, dy) = side.icon_offset();
                cx.fake_item(&tab.icon, left + x + dx, top + y + dy);
            }
        }
        let title = match selected.and_then(|i| self.tabs.get(i)) {
            Some(tab) => tab.title.clone(),
            None => Component::translatable("gui.advancements", Vec::new()),
        };
        cx.frame.component(
            &title,
            (left + 8) as f32,
            (top + 6) as f32,
            0xFF40_4040,
            false,
        );
    }

    fn render_tooltips(&mut self, cx: &mut RenderCx<'_, '_>, left: i32, top: i32) {
        let Some(index) = self.selected else {
            return;
        };
        let (wl, wt) = (left + INSIDE_X, top + INSIDE_Y);
        let gui_w = self.gui.0;
        let hovered = self.tabs[index].hovered;
        let fade = self.tabs[index].fade;
        let alpha = (fade * 255.0).floor() as u32;
        if alpha > 0 {
            cx.frame
                .fill_rect(wl, wt, wl + INSIDE_W, wt + INSIDE_H, alpha << 24);
        }
        if let Some(node_index) = hovered {
            if self.tabs[index].nodes[node_index].hover.is_none() {
                let layout = Self::hover_layout(cx, &self.tabs[index].nodes[node_index]);
                self.tabs[index].nodes[node_index].hover = Some(layout);
            }
            let tab = &self.tabs[index];
            let (ox, oy) = tab.offset();
            draw_hover(
                cx,
                &tab.nodes[node_index],
                (wl + ox, wt + oy),
                (left, gui_w, ox),
                wt,
            );
        }
        if self.tabs.len() > 1 {
            let (mx, my) = (self.mouse.0 as i32, self.mouse.1 as i32);
            for (i, tab) in self.tabs.iter().enumerate() {
                let Some((side, local, _)) = TabSide::of(i) else {
                    continue;
                };
                let (x, y) = side.position(local);
                let (w, h) = side.size();
                if mx > left + x && mx < left + x + w && my > top + y && my < top + y + h {
                    let text = tab.title.to_plain_string();
                    cx.set_tooltip(super::tooltip::TooltipLines::text(text));
                }
            }
        }
    }
}

fn build_tabs(view: &dyn GameView) -> Vec<Tab> {
    let defs = view.advancement_defs();
    let index: std::collections::HashMap<&str, usize> = defs
        .iter()
        .enumerate()
        .map(|(i, d)| (d.id.as_str(), i))
        .collect();
    let parent_of = |d: &PositionedAdvancement| {
        d.advancement
            .parent
            .as_ref()
            .and_then(|p| index.get(p.as_str()).copied())
    };
    let root_of = |mut i: usize| {
        for _ in 0..MAX_PARENT_DEPTH {
            match parent_of(defs[i]) {
                Some(p) => i = p,
                None => return Some(i),
            }
        }
        None
    };
    let visible_parent = |i: usize| {
        let mut cur = parent_of(defs[i]);
        for _ in 0..MAX_PARENT_DEPTH {
            let p = cur?;
            if defs[p].advancement.display.is_some() {
                return Some(p);
            }
            cur = parent_of(defs[p]);
        }
        None
    };
    let mut tabs: Vec<Tab> = Vec::new();
    let mut tab_of_root: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for (i, d) in defs.iter().enumerate() {
        if d.advancement.parent.is_some() {
            continue;
        }
        let Some(display) = &d.advancement.display else {
            continue;
        };
        let Some(background) = &display.background else {
            continue;
        };
        if tabs.len() >= 18 {
            break;
        }
        tab_of_root.insert(i, tabs.len());
        tabs.push(Tab {
            id: d.id.as_str().to_owned(),
            title: display.title.clone(),
            icon: crate_stack(&display.icon),
            background: background_name(background.as_str()),
            nodes: Vec::new(),
            scroll: (0.0, 0.0),
            min: (i32::MAX, i32::MAX),
            max: (i32::MIN, i32::MIN),
            centered: false,
            fade: 0.0,
            hovered: None,
        });
    }
    let mut slot_of: std::collections::HashMap<usize, (usize, usize)> =
        std::collections::HashMap::new();
    for (i, d) in defs.iter().enumerate() {
        let Some(display) = &d.advancement.display else {
            continue;
        };
        let Some(tab_index) = root_of(i).and_then(|r| tab_of_root.get(&r).copied()) else {
            continue;
        };
        let tab = &mut tabs[tab_index];
        let (x, y) = (
            (sane(d.x) * 28.0).floor() as i32,
            (sane(d.y) * 27.0).floor() as i32,
        );
        if tab.nodes.len() >= MAX_NODES_PER_TAB {
            continue;
        }
        tab.min = (tab.min.0.min(x), tab.min.1.min(y));
        tab.max = (tab.max.0.max(x + 28), tab.max.1.max(y + 27));
        let progress = view.advancement_progress(d.id.as_str());
        let total = d.advancement.requirements.len().min(256);
        let completed = progress.map_or(0, |p| {
            d.advancement
                .requirements
                .iter()
                .take(256)
                .filter(|group| group.iter().any(|c| matches!(p.get(c), Some(Some(_)))))
                .count()
        });
        slot_of.insert(i, (tab_index, tab.nodes.len()));
        tab.nodes.push(Node {
            parent: None,
            x,
            y,
            title: display.title.clone(),
            description: display.description.clone(),
            icon: crate_stack(&display.icon),
            frame: display.frame,
            hidden: display.hidden,
            total,
            completed,
            has_progress: progress.is_some(),
            hover: None,
        });
    }
    for (i, &(tab_index, node_index)) in &slot_of {
        if let Some(&(parent_tab, parent_node)) = visible_parent(*i).and_then(|p| slot_of.get(&p))
            && parent_tab == tab_index
        {
            tabs[tab_index].nodes[node_index].parent = Some(parent_node);
        }
    }
    tabs.retain(|t| !t.nodes.is_empty());
    tabs
}

/// Vanilla's `Mth.clamp`: the lower bound wins when the bounds cross, where `f32::clamp` would panic.
fn mth_clamp(v: f32, low: f32, high: f32) -> f32 {
    if v < low { low } else { v.min(high) }
}

fn sane(v: f32) -> f32 {
    if v.is_finite() {
        v.clamp(-MAX_COORD, MAX_COORD)
    } else {
        0.0
    }
}

fn crate_stack(icon: &mcr_protocol::types::item::ItemStackTemplate) -> ItemStack {
    ItemStack {
        item: icon.item,
        count: icon.count,
        components: icon.components.clone(),
    }
}

/// Vanilla background ids name the texture under `textures/gui`, which is how screen assets are keyed.
fn background_name(id: &str) -> Option<String> {
    let path = id.strip_prefix("minecraft:").unwrap_or(id);
    let path = path.strip_prefix("gui/").unwrap_or(path);
    let path = path.strip_suffix(".png").unwrap_or(path);
    path.starts_with("advancements/backgrounds/")
        .then(|| path.to_owned())
}

fn optimal_lines(cx: &mut RenderCx<'_, '_>, cells: &[Cell], preferred: i32) -> Vec<Range<usize>> {
    let mut best: Option<Vec<Range<usize>>> = None;
    let mut best_distance = f32::MAX;
    for margin in SPLIT_OFFSETS {
        let lines = cx
            .frame
            .ui()
            .wrap(cells, (preferred - margin).max(1) as f32);
        let widest = lines
            .iter()
            .map(|r| cx.frame.ui().width(&cells[r.clone()]))
            .fold(0.0f32, f32::max);
        let distance = (widest - preferred as f32).abs();
        if distance <= 10.0 {
            return lines;
        }
        if distance < best_distance {
            best_distance = distance;
            best = Some(lines);
        }
    }
    best.unwrap_or_default()
}

fn hline(cx: &mut RenderCx<'_, '_>, a: i32, b: i32, y: i32, color: u32) {
    let (x0, x1) = (a.min(b), a.max(b));
    cx.frame.fill_rect(x0, y, x1 + 1, y + 1, color);
}

fn vline(cx: &mut RenderCx<'_, '_>, x: i32, a: i32, b: i32, color: u32) {
    let (y0, y1) = (a.min(b), a.max(b));
    cx.frame.fill_rect(x, y0, x + 1, y1 + 1, color);
}

fn connect(
    cx: &mut RenderCx<'_, '_>,
    (xo, yo): (i32, i32),
    parent: &Node,
    node: &Node,
    background: bool,
) {
    let dep_x = xo + parent.x + 13;
    let split_x = xo + parent.x + 26 + 4;
    let dep_y = yo + parent.y + 13;
    let my_x = xo + node.x + 13;
    let my_y = yo + node.y + 13;
    if background {
        let c = 0xFF00_0000;
        hline(cx, split_x, dep_x, dep_y - 1, c);
        hline(cx, split_x + 1, dep_x, dep_y, c);
        hline(cx, split_x, dep_x, dep_y + 1, c);
        hline(cx, my_x, split_x - 1, my_y - 1, c);
        hline(cx, my_x, split_x - 1, my_y, c);
        hline(cx, my_x, split_x - 1, my_y + 1, c);
        vline(cx, split_x - 1, my_y, dep_y, c);
        vline(cx, split_x + 1, my_y, dep_y, c);
    } else {
        let c = WHITE;
        hline(cx, split_x, dep_x, dep_y, c);
        hline(cx, my_x, split_x, my_y, c);
        vline(cx, split_x, my_y, dep_y, c);
    }
}

fn draw_hover(
    cx: &mut RenderCx<'_, '_>,
    node: &Node,
    (xo, yo): (i32, i32),
    (screen_left, screen_width, scroll_x): (i32, i32, i32),
    inside_top: i32,
) {
    let Some(layout) = &node.hover else {
        return;
    };
    let width = layout.width;
    let title_height = 9 * layout.title_lines.len() as i32 + 9 + 8;
    let title_top = yo + node.y + (26 - title_height) / 2;
    let title_bottom = title_top + title_height;
    let description_text = layout.description_lines.len() as i32 * 9;
    let description_height = 6 + description_text;
    let left_side = screen_left + scroll_x + node.x + width + 26 >= screen_width;
    let top_side = title_bottom - inside_top + description_height >= INSIDE_H;
    let progress_text = (node.has_progress && node.total > 1).then(|| {
        cx.lang_fmt(
            "advancements.progress",
            &[&node.completed.to_string(), &node.total.to_string()],
        )
    });
    let progress_width = progress_text.as_deref().map_or(0, |t| cx.text_width(t));
    let amount = node.percent();
    let mut first_width = (amount * width as f32).floor() as i32;
    let (first, second, icon_obtained);
    if amount >= 1.0 {
        first_width = width / 2;
        (first, second, icon_obtained) = (true, true, true);
    } else if first_width < 2 {
        first_width = width / 2;
        (first, second, icon_obtained) = (false, false, false);
    } else if first_width > width - 2 {
        first_width = width / 2;
        (first, second, icon_obtained) = (true, true, false);
    } else {
        (first, second, icon_obtained) = (true, false, false);
    }
    let title_left = if left_side {
        xo + node.x - width + 26 + 6
    } else {
        xo + node.x
    };
    let background_height = title_height + description_height;
    if !layout.description_lines.is_empty() {
        let y = if top_side {
            title_bottom - background_height
        } else {
            title_top
        };
        cx.sprite(
            "advancements/title_box",
            title_left,
            y,
            width,
            background_height,
        );
    }
    let box_sprite = |obtained: bool| {
        if obtained {
            "advancements/box_obtained"
        } else {
            "advancements/box_unobtained"
        }
    };
    if first != second {
        cx.frame.push_scissor(
            title_left,
            title_top,
            title_left + first_width,
            title_top + title_height,
        );
        cx.sprite(
            box_sprite(first),
            title_left,
            title_top,
            width,
            title_height,
        );
        cx.frame.pop_scissor();
        cx.frame.push_scissor(
            title_left + first_width,
            title_top,
            title_left + width,
            title_top + title_height,
        );
        cx.sprite(
            box_sprite(second),
            title_left,
            title_top,
            width,
            title_height,
        );
        cx.frame.pop_scissor();
    } else {
        cx.sprite(
            box_sprite(first),
            title_left,
            title_top,
            width,
            title_height,
        );
    }
    cx.sprite(
        node.frame_sprite(icon_obtained),
        xo + node.x + 3,
        yo + node.y,
        26,
        26,
    );
    let description_left = title_left + 5;
    let title_x = if left_side {
        description_left
    } else {
        xo + node.x + 32
    };
    for (i, range) in layout.title_lines.iter().enumerate() {
        cx.frame.cells(
            &layout.title_cells[range.clone()],
            title_x as f32,
            (title_top + 9 + i as i32 * 9) as f32,
            WHITE,
            true,
        );
    }
    if let Some(text) = &progress_text {
        let x = if left_side {
            xo + node.x - progress_width
        } else {
            xo + node.x + width - progress_width - 5
        };
        cx.text(text, x, title_top + 9, WHITE, true);
    }
    let description_y = if top_side {
        title_top - description_text + 1
    } else {
        title_bottom
    };
    for (i, range) in layout.description_lines.iter().enumerate() {
        cx.frame.cells(
            &layout.description_cells[range.clone()],
            description_left as f32,
            (description_y + i as i32 * 9) as f32,
            node.type_color(),
            true,
        );
    }
    cx.stack(&node.icon, xo + node.x + 8, yo + node.y + 5);
}

impl Screen for AdvancementsScreen {
    fn init(&mut self, width: i32, height: i32) {
        self.gui = (width, height);
        self.widgets.clear();
        self.widgets.add(Button::new(
            DONE,
            [width / 2 - 100, height - 27, 200, 20],
            "lang:gui.done",
        ));
    }

    fn pauses_game(&self) -> bool {
        true
    }

    fn tick(&mut self, env: &mut Env<'_>, out: &mut Vec<ScreenAction>) {
        self.sync(env.view);
        let (left, top) = self.window();
        let (mx, my) = (
            self.mouse.0 as i32 - left - INSIDE_X,
            self.mouse.1 as i32 - top - INSIDE_Y,
        );
        if let Some(tab) = self.selected.and_then(|i| self.tabs.get_mut(i)) {
            let hit = tab.node_at(mx, my);
            if hit.is_some() {
                tab.hovered = hit;
                tab.fade = (tab.fade + 0.06).clamp(0.0, 0.3);
            } else {
                tab.fade = (tab.fade - 0.12).clamp(0.0, 1.0);
                tab.hovered = None;
            }
        }
        out.append(&mut self.pending);
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        self.mouse = cx.mouse;
        self.sync(cx.view);
        draw_menu_background(cx);
        let (gw, _) = cx.gui_size();
        let (left, top) = self.window();
        let title = cx.lang("gui.advancements");
        cx.centered_text(&title, gw / 2, 12, WHITE, true);
        self.render_inside(cx, left, top);
        self.render_window(cx, left, top);
        self.render_tooltips(cx, left, top);
        self.widgets.render(cx);
    }

    fn input(&mut self, env: &mut Env<'_>, input: &UiInput, out: &mut Vec<ScreenAction>) -> bool {
        match *input {
            UiInput::Key {
                key: Key::Escape | Key::Letter('l'),
                ..
            } => {
                self.close(out);
                return true;
            }
            UiInput::MouseMove { x, y } => {
                if self.dragging
                    && let Some(tab) = self.selected.and_then(|i| self.tabs.get_mut(i))
                {
                    tab.scroll_by(x - self.mouse.0, y - self.mouse.1);
                }
                self.mouse = (x, y);
            }
            UiInput::MousePress {
                x,
                y,
                button: MouseButton::Left,
                ..
            } => {
                let mut events = Vec::new();
                if self.widgets.input(env, input, &mut events) {
                    for WidgetEvent { id, kind } in events {
                        if id == DONE && kind == WidgetEventKind::Pressed {
                            out.push(ScreenAction::ClickSound);
                            self.close(out);
                        }
                    }
                    return true;
                }
                if let Some(index) = self.tab_at(x, y) {
                    self.select(index);
                    return true;
                }
                self.dragging = true;
                self.mouse = (x, y);
                return true;
            }
            UiInput::MouseRelease { .. } => {
                self.dragging = false;
            }
            UiInput::Scroll { dx, dy, .. } => {
                if let Some(tab) = self.selected.and_then(|i| self.tabs.get_mut(i)) {
                    tab.scroll_by(dx * SCROLL_SPEED, dy * SCROLL_SPEED);
                    return true;
                }
                return false;
            }
            _ => {}
        }
        let mut events = Vec::new();
        let used = self.widgets.input(env, input, &mut events);
        for WidgetEvent { id, kind } in events {
            if id == DONE && kind == WidgetEventKind::Pressed {
                out.push(ScreenAction::ClickSound);
                self.close(out);
            }
        }
        used
    }
}

impl AdvancementsScreen {
    fn tab_at(&self, x: f32, y: f32) -> Option<usize> {
        if self.tabs.len() < 2 {
            return None;
        }
        let (left, top) = self.window();
        (0..self.tabs.len()).find(|&i| {
            let Some((side, local, _)) = TabSide::of(i) else {
                return false;
            };
            let (tx, ty) = side.position(local);
            let (w, h) = side.size();
            x > (left + tx) as f32
                && x < (left + tx + w) as f32
                && y > (top + ty) as f32
                && y < (top + ty + h) as f32
        })
    }
}

#[cfg(test)]
mod tests {
    use mcr_buf::Identifier;
    use mcr_protocol::play::clientbound::ui::{Advancement, DisplayInfo};
    use mcr_protocol::types::item::ItemStackTemplate;
    use rustc_hash::FxHashMap;

    use super::*;

    struct Mock {
        defs: Vec<PositionedAdvancement>,
        progress: FxHashMap<String, FxHashMap<String, Option<i64>>>,
    }

    impl GameView for Mock {
        fn advancement_defs(&self) -> Vec<&PositionedAdvancement> {
            self.defs.iter().collect()
        }

        fn advancement_progress(&self, id: &str) -> Option<&FxHashMap<String, Option<i64>>> {
            self.progress.get(id)
        }
    }

    fn def(
        id: &str,
        parent: Option<&str>,
        x: f32,
        y: f32,
        root: bool,
        groups: usize,
    ) -> PositionedAdvancement {
        PositionedAdvancement {
            id: Identifier::parse(id).unwrap(),
            advancement: Advancement {
                parent: parent.map(|p| Identifier::parse(p).unwrap()),
                display: Some(DisplayInfo {
                    title: Component::translatable("t", Vec::new()),
                    description: Component::translatable("d", Vec::new()),
                    icon: ItemStackTemplate {
                        item: 1,
                        count: 1,
                        components: Default::default(),
                    },
                    frame: AdvancementFrame::Task,
                    background: root.then(|| {
                        Identifier::parse("minecraft:gui/advancements/backgrounds/stone").unwrap()
                    }),
                    show_toast: false,
                    hidden: false,
                }),
                requirements: (0..groups).map(|i| vec![format!("c{i}")]).collect(),
                sends_telemetry_event: false,
            },
            x,
            y,
        }
    }

    #[test]
    fn trees_become_tabs_with_layout_bounds_and_progress() {
        let mut progress = FxHashMap::default();
        progress.insert(
            "a:child".to_owned(),
            FxHashMap::from_iter([("c0".to_owned(), Some(5)), ("c1".to_owned(), None)]),
        );
        let view = Mock {
            defs: vec![
                def("a:root", None, 0.0, 0.0, true, 1),
                def("a:child", Some("a:root"), 1.0, 0.5, false, 2),
                def("b:orphan", Some("missing:parent"), 0.0, 0.0, false, 1),
            ],
            progress,
        };
        let tabs = build_tabs(&view);
        assert_eq!(tabs.len(), 1);
        let tab = &tabs[0];
        assert_eq!(
            tab.background.as_deref(),
            Some("advancements/backgrounds/stone")
        );
        assert_eq!(tab.nodes.len(), 2);
        assert_eq!((tab.nodes[1].x, tab.nodes[1].y), (28, 13));
        assert_eq!(tab.nodes[1].parent, Some(0));
        assert_eq!((tab.nodes[1].completed, tab.nodes[1].total), (1, 2));
        assert!(!tab.nodes[1].done() && tab.nodes[1].percent() == 0.5);
        assert_eq!((tab.min, tab.max), ((0, 0), (56, 40)));
    }

    #[test]
    fn hostile_positions_and_counts_stay_bounded() {
        let mut defs = vec![def("a:root", None, f32::NAN, f32::INFINITY, true, 1)];
        defs.push(def("a:far", Some("a:root"), 3.0e38, -3.0e38, false, 1));
        for i in 0..3000 {
            defs.push(def(
                &format!("a:n{i}"),
                Some("a:root"),
                i as f32,
                0.0,
                false,
                1,
            ));
        }
        let view = Mock {
            defs,
            progress: FxHashMap::default(),
        };
        let tabs = build_tabs(&view);
        assert_eq!(tabs.len(), 1);
        assert!(tabs[0].nodes.len() <= MAX_NODES_PER_TAB);
        assert!(tabs[0].max.0 <= (MAX_COORD * 28.0) as i32 + 28);
        assert_eq!((tabs[0].nodes[0].x, tabs[0].nodes[0].y), (0, 0));
    }

    #[test]
    fn parent_cycles_do_not_hang() {
        let view = Mock {
            defs: vec![
                def("a:x", Some("a:y"), 0.0, 0.0, false, 1),
                def("a:y", Some("a:x"), 0.0, 0.0, false, 1),
            ],
            progress: FxHashMap::default(),
        };
        assert!(build_tabs(&view).is_empty());
    }

    #[test]
    fn tabs_spread_over_the_four_sides() {
        assert_eq!(
            TabSide::of(0).map(|t| (t.0, t.1)),
            Some((TabSide::Above, 0))
        );
        assert_eq!(
            TabSide::of(8).map(|t| (t.0, t.1)),
            Some((TabSide::Below, 0))
        );
        assert_eq!(
            TabSide::of(16).map(|t| (t.0, t.1)),
            Some((TabSide::Left, 0))
        );
        assert_eq!(
            TabSide::of(21).map(|t| (t.0, t.1)),
            Some((TabSide::Right, 0))
        );
        assert!(TabSide::of(26).is_none());
        assert_eq!(
            TabSide::Above.sprite(0, 8, true),
            "advancements/tab_above_left_selected"
        );
        assert_eq!(
            TabSide::Right.sprite(4, 5, false),
            "advancements/tab_right_bottom"
        );
    }

    #[test]
    fn backgrounds_resolve_only_inside_the_gui_folder() {
        assert_eq!(
            background_name("minecraft:gui/advancements/backgrounds/end.png").as_deref(),
            Some("advancements/backgrounds/end")
        );
        assert_eq!(background_name("minecraft:textures/entity/creeper"), None);
    }

    #[test]
    fn scrolling_stays_inside_the_tree() {
        let mut tab = Tab {
            id: String::new(),
            title: Component::translatable("t", Vec::new()),
            icon: ItemStack::default(),
            background: None,
            nodes: Vec::new(),
            scroll: (0.0, 0.0),
            min: (0, 0),
            max: (400, 100),
            centered: false,
            fade: 0.0,
            hovered: None,
        };
        tab.scroll_by(-1000.0, -1000.0);
        assert_eq!(tab.scroll, (-166.0, 0.0));
        tab.scroll_by(1000.0, 1000.0);
        assert_eq!(tab.scroll, (0.0, 0.0));
    }

    #[test]
    fn hostile_positions_and_sizes_are_clamped() {
        let mut defs = vec![def("a:root", None, f32::NAN, f32::INFINITY, true, 1)];
        defs.push(def("a:far", Some("a:root"), 1.0e30, -1.0e30, false, 1));
        let view = Mock {
            defs,
            progress: FxHashMap::default(),
        };
        let tab = &build_tabs(&view)[0];
        assert!(tab.max.0 <= 4096 * 28 + 28 && tab.min.0 >= -4096 * 28);
        assert_eq!(tab.nodes[0].x, 0);
    }

    #[test]
    fn crossed_or_empty_bounds_do_not_panic() {
        let mut defs = vec![def("a:root", None, -4000.0, 0.0, true, 1)];
        defs.push(def("a:near", Some("a:root"), 1.0, 0.0, false, 1));
        let view = Mock {
            defs,
            progress: FxHashMap::default(),
        };
        let mut tab = build_tabs(&view).remove(0);
        tab.scroll_by(-50.0, 0.0);
        tab.scroll_by(1.0e9, 0.0);
        assert!(tab.scroll.0.is_finite());
        tab.nodes.clear();
        tab.min = (i32::MAX, i32::MAX);
        tab.max = (i32::MIN, i32::MIN);
        tab.centered = false;
        tab.center();
        tab.scroll_by(10.0, 10.0);
    }

    #[test]
    fn nodes_per_tab_are_capped() {
        let mut defs = vec![def("a:root", None, 0.0, 0.0, true, 1)];
        for i in 0..(MAX_NODES_PER_TAB + 50) {
            defs.push(def(&format!("a:n{i}"), Some("a:root"), 0.0, 0.0, false, 1));
        }
        let view = Mock {
            defs,
            progress: FxHashMap::default(),
        };
        assert_eq!(build_tabs(&view)[0].nodes.len(), MAX_NODES_PER_TAB);
    }
}
