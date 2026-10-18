use mcr_protocol::types::item::ItemStack;
use mcr_protocol::types::text::ClickEvent;

use super::kind::{FurnaceKind, MenuKind};
use super::screen::{ContainerScreen, LABEL};
use crate::gui::book::PageLayout;
use crate::gui::clicks::click_event_actions;
use crate::gui::edit_box::EditBox;
use crate::gui::env::{Env, GameView, RenderCx, is_empty};
use crate::gui::input::{Key, MouseButton, UiInput, contains};
use crate::gui::screen::ScreenAction;
use crate::gui::tooltip::TooltipLines;
use crate::gui::widgets::{Button, WidgetEvent, WidgetEventKind};
use crate::items::StackView;
use crate::style::{Cell, FontId};

const ANVIL_NAME: u32 = 1;
const BOOK_DONE: u32 = 10;
const BOOK_TAKE: u32 = 11;
const BOOK_PREV: u32 = 12;
const BOOK_NEXT: u32 = 13;
const EXPERIENCE_THRESHOLDS: [i32; 5] = [0, 10, 70, 150, 250];
const BUBBLES: [i32; 7] = [29, 24, 20, 16, 11, 6, 0];
const GALACTIC: &str = "the elder scrolls klaatu berata niktu xyzzy bless curse light darkness fire air earth water hot dry cold wet ignite snuff embiggen twist shorten stretch fiddle destroy imbue galvanize enchant free limited range of towards inside sphere cube self other ball mental physical grow shrink demon elemental spirit animal creature beast humanoid undead fresh stale phnglui mglwnafh cthulhu rlyeh wgahnagl fhtagn baguette";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BeaconKind {
    Power { primary: bool, tier: usize },
    Upgrade,
    Confirm,
    Cancel,
}

#[derive(Clone, Copy, Debug)]
struct BeaconButton {
    rect: [i32; 4],
    kind: BeaconKind,
    effect: Option<&'static str>,
}

/// Beacon effect names with the `data(1)` / `data(2)` network ids (registry id + 1).
const BEACON_EFFECTS: [(&str, i32); 6] = [
    ("speed", 1),
    ("haste", 3),
    ("resistance", 11),
    ("jump_boost", 8),
    ("strength", 5),
    ("regeneration", 10),
];

fn effect_by_id(id: i32) -> Option<&'static str> {
    BEACON_EFFECTS.iter().find(|e| e.1 == id).map(|e| e.0)
}

#[derive(Clone, Debug, Default)]
pub struct BeaconState {
    buttons: Vec<BeaconButtonOwned>,
    primary: Option<&'static str>,
    secondary: Option<&'static str>,
    synced: Option<(i32, i32)>,
}

#[derive(Clone, Copy, Debug)]
struct BeaconButtonOwned(BeaconButton);

impl ContainerScreen {
    pub(super) fn setup_widgets(&mut self) {
        self.widgets.clear();
        self.beacon.buttons.clear();
        match self.layout.kind {
            MenuKind::Anvil => {
                let mut name = EditBox::new(ANVIL_NAME, [self.left + 62, self.top + 24, 103, 12])
                    .with_max_length(50);
                name.bordered = false;
                name.text_color = 0xFFFF_FFFF;
                name.editable = false;
                self.widgets.add(name);
                self.widgets.set_focus(Some(ANVIL_NAME), 0);
                self.anvil_item = None;
                self.anvil_sent = None;
            }
            MenuKind::Beacon => self.build_beacon(),
            MenuKind::Lectern => {
                let middle = self.gui.0 / 2;
                let y = 2 + 192 + 2;
                self.widgets
                    .add(Button::new(BOOK_DONE, [middle - 100, y, 98, 20], "Done"));
                self.widgets
                    .add(Button::new(BOOK_TAKE, [middle + 2, y, 98, 20], "Take Book"));
                let left = (self.gui.0 - 192) / 2;
                self.widgets
                    .add(Button::new(BOOK_PREV, [left + 43, 2 + 157, 23, 13], "").flat());
                self.widgets
                    .add(Button::new(BOOK_NEXT, [left + 116, 2 + 157, 23, 13], "").flat());
            }
            _ => {}
        }
    }

    fn build_beacon(&mut self) {
        let (l, t) = (self.left, self.top);
        let mut push = |rect: [i32; 4], kind, effect| {
            self.beacon
                .buttons
                .push(BeaconButtonOwned(BeaconButton { rect, kind, effect }));
        };
        let tiers: [&[&'static str]; 3] = [
            &["speed", "haste"],
            &["resistance", "jump_boost"],
            &["strength"],
        ];
        for (tier, effects) in tiers.iter().enumerate() {
            let count = effects.len() as i32;
            let total = count * 22 + (count - 1) * 2;
            for (c, effect) in effects.iter().enumerate() {
                push(
                    [
                        l + 76 + c as i32 * 24 - total / 2,
                        t + 22 + tier as i32 * 25,
                        22,
                        22,
                    ],
                    BeaconKind::Power {
                        primary: true,
                        tier,
                    },
                    Some(*effect),
                );
            }
        }
        let count = 2;
        let total = count * 22 + (count - 1) * 2;
        push(
            [l + 167 - total / 2, t + 47, 22, 22],
            BeaconKind::Power {
                primary: false,
                tier: 3,
            },
            Some("regeneration"),
        );
        push(
            [l + 167 + 24 - total / 2, t + 47, 22, 22],
            BeaconKind::Upgrade,
            None,
        );
        push([l + 164, t + 107, 22, 22], BeaconKind::Confirm, None);
        push([l + 190, t + 107, 22, 22], BeaconKind::Cancel, None);
    }

    fn beacon_selected(&self, b: &BeaconButton) -> bool {
        match b.kind {
            BeaconKind::Power { primary: true, .. } => b.effect == self.beacon.primary,
            BeaconKind::Power { primary: false, .. } => b.effect == self.beacon.secondary,
            BeaconKind::Upgrade => {
                self.beacon.primary.is_some() && self.beacon.primary == self.beacon.secondary
            }
            _ => false,
        }
    }

    fn beacon_active(&self, view: &dyn GameView, b: &BeaconButton) -> bool {
        let levels = view.data(0);
        match b.kind {
            BeaconKind::Power { tier, .. } => (tier as i32) < levels,
            BeaconKind::Upgrade => self.beacon.primary.is_some() && levels > 3,
            BeaconKind::Confirm => !is_empty(view.slot(0)) && self.beacon.primary.is_some(),
            BeaconKind::Cancel => true,
        }
    }

    fn beacon_visible(&self, b: &BeaconButton) -> bool {
        b.kind != BeaconKind::Upgrade || self.beacon.primary.is_some()
    }

    pub(super) fn sync(&mut self, cx: &mut RenderCx<'_, '_>) {
        let view = cx.view;
        for (i, ov) in self.crafter_override.iter_mut().enumerate() {
            if *ov == Some(view.data(i) == 1) {
                *ov = None;
            }
        }
        match self.layout.kind {
            MenuKind::Beacon => {
                let now = (view.data(1), view.data(2));
                if self.beacon.synced != Some(now) {
                    self.beacon.synced = Some(now);
                    self.beacon.primary = effect_by_id(now.0);
                    self.beacon.secondary = effect_by_id(now.1);
                }
            }
            MenuKind::Anvil => {
                let current = view.slot(0).filter(|s| !s.is_empty()).cloned();
                if current != self.anvil_item {
                    let name = current
                        .as_ref()
                        .map(|s| cx.item_tooltip(s).first_plain())
                        .unwrap_or_default();
                    if let Some(e) = self.widgets.edit_mut(ANVIL_NAME) {
                        e.set_value(&name);
                        e.take_changed();
                        e.editable = current.is_some();
                    }
                    self.anvil_hover = name.clone();
                    self.anvil_item = current;
                    self.widgets.set_focus(Some(ANVIL_NAME), cx.now_ms);
                    if self.anvil_item.is_some() {
                        self.send_rename(&name);
                    }
                }
            }
            _ => {}
        }
    }

    /// Vanilla sends an empty name when the box still shows the item's own, uncustomised name.
    fn rename_payload(&self, typed: &str) -> Option<String> {
        let item = self.anvil_item.as_ref()?;
        let named = mcr_protocol::types::component::component_type_id("custom_name")
            .is_some_and(|id| item.components.get(id).is_some());
        let payload = if !named && typed == self.anvil_hover {
            String::new()
        } else {
            typed.to_owned()
        };
        (self.anvil_sent.as_deref() != Some(&payload)).then_some(payload)
    }

    fn send_rename(&mut self, typed: &str) {
        if let Some(payload) = self.rename_payload(typed) {
            self.anvil_sent = Some(payload.clone());
            self.pending_rename = Some(payload);
        }
    }

    pub(super) fn widget_events(&mut self, events: &[WidgetEvent], out: &mut Vec<ScreenAction>) {
        for event in events {
            match (self.layout.kind, event.id, event.kind) {
                (MenuKind::Anvil, ANVIL_NAME, WidgetEventKind::Edited) => {
                    if let Some(e) = self.widgets.edit(ANVIL_NAME) {
                        let value = e.value().to_owned();
                        self.send_rename(&value);
                        out.extend(self.pending_rename.take().map(ScreenAction::RenameItem));
                    }
                }
                (MenuKind::Lectern, BOOK_DONE, WidgetEventKind::Pressed) => {
                    out.push(ScreenAction::CloseContainer);
                }
                (MenuKind::Lectern, BOOK_TAKE, WidgetEventKind::Pressed) => {
                    out.push(ScreenAction::ContainerButton(3));
                }
                (MenuKind::Lectern, BOOK_PREV, WidgetEventKind::Pressed) => {
                    out.push(ScreenAction::ContainerButton(1));
                }
                (MenuKind::Lectern, BOOK_NEXT, WidgetEventKind::Pressed) => {
                    out.push(ScreenAction::ContainerButton(2));
                }
                _ => {}
            }
        }
    }

    pub(super) fn pre_press(
        &mut self,
        env: &mut Env<'_>,
        (x, y): (f32, f32),
        button: MouseButton,
        out: &mut Vec<ScreenAction>,
    ) -> bool {
        let view = env.view;
        let input = UiInput::press(x, y, button);
        let mut events = Vec::new();
        let used = self.widgets.input(env, &input, &mut events);
        self.widget_events(&events, out);
        if self.layout.kind == MenuKind::Anvil {
            self.widgets.set_focus(Some(ANVIL_NAME), env.now_ms);
        }
        if used {
            return true;
        }
        if button != MouseButton::Left {
            return false;
        }
        match self.layout.kind {
            MenuKind::Enchantment => {
                for i in 0..3usize {
                    if self.inside([60, 14 + 19 * i as i32, 108, 19], x, y)
                        && self.enchant_clickable(view, i)
                    {
                        out.push(ScreenAction::ContainerButton(i as i32));
                        return true;
                    }
                }
                false
            }
            MenuKind::Stonecutter => {
                let count = view.stonecutter_options().len().min(1024);
                for index in self.list.start..(self.list.start + 12).min(count) {
                    let pos = index - self.list.start;
                    if self.inside(
                        [
                            52 + (pos % 4) as i32 * 16,
                            14 + (pos / 4) as i32 * 18,
                            16,
                            18,
                        ],
                        x,
                        y,
                    ) {
                        out.push(ScreenAction::ContainerButton(index as i32));
                        out.push(ScreenAction::ClickSound);
                        return true;
                    }
                }
                if count > 0 && self.inside([119, 9, 12, 54], x, y) {
                    self.list.dragging = true;
                }
                false
            }
            MenuKind::Loom => {
                let count = view.loom_pattern_count().min(1024);
                if self.loom_patterns_shown(view) {
                    for row in 0..4usize {
                        for col in 0..4usize {
                            let index = (row + self.list.start) * 4 + col;
                            if index < count
                                && self.inside(
                                    [60 + col as i32 * 14, 13 + row as i32 * 14, 14, 14],
                                    x,
                                    y,
                                )
                            {
                                out.push(ScreenAction::ContainerButton(index as i32));
                                out.push(ScreenAction::ClickSound);
                                return true;
                            }
                        }
                    }
                    if self.inside([119, 9, 12, 56], x, y) {
                        self.list.dragging = true;
                    }
                }
                false
            }
            MenuKind::Merchant => {
                let offers = view.merchant().map_or(0, |m| m.offers.len().min(1024));
                for i in 0..7usize {
                    if i < offers && self.inside([5, 18 + 20 * i as i32, 88, 20], x, y) {
                        self.merchant_selected = i + self.merchant_scroll;
                        out.push(ScreenAction::SelectTrade(self.merchant_selected));
                        out.push(ScreenAction::ClickSound);
                        return true;
                    }
                }
                if offers > 7 && self.inside([94, 18, 6, 140], x, y) {
                    self.list.dragging = true;
                }
                false
            }
            MenuKind::Beacon => {
                let hit = self.beacon.buttons.iter().map(|b| b.0).find(|b| {
                    self.beacon_visible(b) && self.beacon_active(view, b) && contains(b.rect, x, y)
                });
                let Some(b) = hit else {
                    return false;
                };
                out.push(ScreenAction::ClickSound);
                match b.kind {
                    BeaconKind::Power { primary, .. } => {
                        if !self.beacon_selected(&b) {
                            if primary {
                                self.beacon.primary = b.effect;
                            } else {
                                self.beacon.secondary = b.effect;
                            }
                        }
                    }
                    BeaconKind::Upgrade => self.beacon.secondary = self.beacon.primary,
                    BeaconKind::Confirm => {
                        out.push(ScreenAction::SetBeacon {
                            primary: self.beacon.primary.map(|e| format!("minecraft:{e}")),
                            secondary: self.beacon.secondary.map(|e| format!("minecraft:{e}")),
                        });
                        out.push(ScreenAction::CloseContainer);
                    }
                    BeaconKind::Cancel => out.push(ScreenAction::CloseContainer),
                }
                true
            }
            _ => false,
        }
    }

    fn enchant_clickable(&self, view: &dyn GameView, option: usize) -> bool {
        let cost = view.data(option);
        let lapis = view.slot(1).map_or(0, |s| s.count.max(0));
        let enough = view.creative() || (lapis > option as i32 && view.xp_level() >= cost);
        cost > 0 && !is_empty(view.slot(0)) && enough
    }

    fn loom_patterns_shown(&self, view: &dyn GameView) -> bool {
        !is_empty(view.slot(0)) && !is_empty(view.slot(1)) && view.loom_pattern_count() > 0
    }

    fn scroll_extent(&self, view: &dyn GameView) -> Option<(usize, usize, f32, f32, usize)> {
        match self.layout.kind {
            MenuKind::Stonecutter => {
                let n = view.stonecutter_options().len().min(1024);
                let rows = n.div_ceil(4);
                (n > 12).then(|| (rows - 3, 4, 14.0, 54.0, 4))
            }
            MenuKind::Loom => {
                let n = view.loom_pattern_count().min(1024);
                let rows = n.div_ceil(4);
                (self.loom_patterns_shown(view) && n > 16).then(|| (rows - 4, 1, 13.0, 56.0, 1))
            }
            _ => None,
        }
    }

    pub(super) fn drag_scroll(&mut self, view: &dyn GameView, y: f32) {
        if !self.list.dragging {
            return;
        }
        if self.layout.kind == MenuKind::Merchant {
            let offers = view.merchant().map_or(0, |m| m.offers.len().min(1024));
            if offers > 7 {
                let top = (self.top + 18) as f32;
                let max = (offers - 7) as f32;
                let v = ((y - top - 13.5) / (139.0 - 27.0)) * max + 0.5;
                self.merchant_scroll = (v.max(0.0) as usize).min(offers - 7);
            }
            return;
        }
        let Some((offscreen, per_row, top, height, _)) = self.scroll_extent(view) else {
            return;
        };
        let y0 = self.top as f32 + top;
        self.list.offset = ((y - y0 - 7.5) / (height - 15.0)).clamp(0.0, 1.0);
        self.list.start = (self.list.offset * offscreen as f32 + 0.5) as usize * per_row;
    }

    pub(super) fn scroll(&mut self, view: &dyn GameView, dy: f32) -> bool {
        if self.layout.kind == MenuKind::Merchant {
            let offers = view.merchant().map_or(0, |m| m.offers.len().min(1024));
            if offers > 7 {
                let max = (offers - 7) as i64;
                self.merchant_scroll =
                    (self.merchant_scroll as i64 - dy as i64).clamp(0, max) as usize;
            }
            return true;
        }
        if let Some((offscreen, per_row, ..)) = self.scroll_extent(view) {
            self.list.offset = (self.list.offset - dy / offscreen as f32).clamp(0.0, 1.0);
            self.list.start = (self.list.offset * offscreen as f32 + 0.5) as usize * per_row;
        }
        true
    }

    fn texture_window(&self, cx: &mut RenderCx<'_, '_>) {
        let (l, t, w, h) = (self.left, self.top, self.layout.width, self.layout.height);
        if let MenuKind::Generic { rows } = self.layout.kind {
            let rows = i32::from(rows.clamp(1, 6));
            cx.texture(self.layout.texture, (0, 0), (l, t, w, rows * 18 + 17));
            cx.texture(
                self.layout.texture,
                (0, 126),
                (l, t + rows * 18 + 17, w, 96),
            );
        } else {
            cx.texture(self.layout.texture, (0, 0), (l, t, w, h));
        }
    }

    pub(super) fn draw_background(&mut self, cx: &mut RenderCx<'_, '_>) {
        self.texture_window(cx);
        let view = cx.view;
        let (l, t) = (self.left, self.top);
        match self.layout.kind {
            MenuKind::Furnace(kind) => self.draw_furnace(cx, kind),
            MenuKind::BrewingStand => self.draw_brewing(cx),
            MenuKind::Enchantment => self.draw_enchanting(cx),
            MenuKind::Anvil => {
                let sprite = if is_empty(view.slot(0)) {
                    "container/anvil/text_field_disabled"
                } else {
                    "container/anvil/text_field"
                };
                cx.sprite(sprite, l + 59, t + 20, 110, 16);
                if (!is_empty(view.slot(0)) || !is_empty(view.slot(1))) && is_empty(view.slot(2)) {
                    cx.sprite("container/anvil/error", l + 99, t + 45, 28, 21);
                }
            }
            MenuKind::Beacon => self.draw_beacon(cx),
            MenuKind::Grindstone => {
                if (!is_empty(view.slot(0)) || !is_empty(view.slot(1))) && is_empty(view.slot(2)) {
                    cx.sprite("container/grindstone/error", l + 92, t + 31, 28, 21);
                }
            }
            MenuKind::CartographyTable => self.draw_cartography(cx),
            MenuKind::Smithing => {
                if view.data(0) == 1 {
                    cx.sprite("container/smithing/error", l + 65, t + 46, 28, 21);
                }
            }
            MenuKind::Loom => self.draw_loom(cx),
            MenuKind::Stonecutter => self.draw_stonecutter(cx),
            MenuKind::Merchant => self.draw_merchant_background(cx),
            MenuKind::Crafter => {
                let sprite = if view.data(9) == 1 {
                    "container/crafter/powered_redstone"
                } else {
                    "container/crafter/unpowered_redstone"
                };
                let (gw, gh) = cx.gui_size();
                cx.sprite(sprite, gw / 2 + 9, gh / 2 - 48, 16, 16);
            }
            MenuKind::Mount {
                chest_columns,
                saddle,
                armor,
                llama,
            } => {
                if chest_columns > 0
                    && let Some(id) = cx.sprite_id("container/horse/chest_slots")
                {
                    let w = i32::from(chest_columns.min(5)) * 18;
                    cx.frame.sprite_region(id, (0, 0), (l + 79, t + 17, w, 54));
                }
                if saddle {
                    cx.sprite("container/slot", l + 7, t + 17, 18, 18);
                }
                if armor || llama {
                    cx.sprite("container/slot", l + 7, t + 35, 18, 18);
                }
            }
            MenuKind::Nautilus { saddle, armor } => {
                if saddle {
                    cx.sprite("container/slot", l + 7, t + 17, 18, 18);
                }
                if armor {
                    cx.sprite("container/slot", l + 7, t + 35, 18, 18);
                }
            }
            _ => {}
        }
    }

    pub(super) fn draw_foreground(&mut self, cx: &mut RenderCx<'_, '_>) {
        match self.layout.kind {
            MenuKind::Merchant => self.draw_merchant_foreground(cx),
            MenuKind::Anvil => self.draw_anvil_cost(cx),
            MenuKind::Crafter
                if self.hovered_slot().is_some_and(|h| {
                    h < 9
                        && is_empty(cx.view.slot(h))
                        && is_empty(cx.view.carried())
                        && !cx.view.spectator()
                        && cx.view.data(h) != 1
                }) =>
            {
                let text = cx.lang("gui.togglable_slot");
                cx.set_tooltip(TooltipLines::text(text));
            }
            _ => {}
        }
    }

    fn region(
        &self,
        cx: &mut RenderCx<'_, '_>,
        name: &str,
        src: (i32, i32),
        dst: (i32, i32, i32, i32),
    ) {
        if let Some(id) = cx.sprite_id(name) {
            cx.frame.sprite_region(id, src, dst);
        }
    }

    fn draw_furnace(&self, cx: &mut RenderCx<'_, '_>, kind: FurnaceKind) {
        let view = cx.view;
        let (l, t) = (self.left, self.top);
        let dir = kind.sprite_dir();
        if view.data(0) > 0 {
            let duration = match view.data(1) {
                0 => 200,
                d => d,
            };
            let progress = fraction(view.data(0), duration);
            let h = (progress * 13.0).ceil() as i32 + 1;
            self.region(
                cx,
                &format!("{dir}/lit_progress"),
                (0, 14 - h),
                (l + 56, t + 36 + 14 - h, 14, h),
            );
        }
        let (cooked, total) = (view.data(2), view.data(3));
        let burn = fraction(cooked, total);
        let w = (burn * 24.0).ceil() as i32;
        if w > 0 {
            self.region(
                cx,
                &format!("{dir}/burn_progress"),
                (0, 0),
                (l + 79, t + 34, w, 16),
            );
        }
    }

    fn draw_brewing(&self, cx: &mut RenderCx<'_, '_>) {
        let view = cx.view;
        let (l, t) = (self.left, self.top);
        let (fuel, total_fuel) = (view.data(1), view.data(3));
        let fuel_len = if total_fuel > 0 {
            ((18 * i64::from(fuel) + i64::from(total_fuel) - 1) / i64::from(total_fuel))
                .clamp(0, 18) as i32
        } else {
            0
        };
        if fuel_len > 0 {
            self.region(
                cx,
                "container/brewing_stand/fuel_length",
                (0, 0),
                (l + 60, t + 44, fuel_len, 4),
            );
        }
        let (ticks, total) = (view.data(0), view.data(2));
        if ticks > 0 && total > 0 {
            let len = (28.0 * (1.0 - fraction(ticks, total))) as i32;
            if len > 0 {
                self.region(
                    cx,
                    "container/brewing_stand/brew_progress",
                    (0, 0),
                    (l + 97, t + 16, 9, len),
                );
            }
            let len = BUBBLES[(ticks / 2 % 7) as usize];
            if len > 0 {
                self.region(
                    cx,
                    "container/brewing_stand/bubbles",
                    (0, 29 - len),
                    (l + 63, t + 14 + 29 - len, 12, len),
                );
            }
        }
    }

    fn galactic_name(
        seed: i32,
        option: usize,
        max_width: i32,
        cx: &mut RenderCx<'_, '_>,
    ) -> Vec<Cell> {
        let words: Vec<&str> = GALACTIC.split(' ').collect();
        let mut state = (seed as u32)
            .wrapping_mul(2_654_435_761)
            .wrapping_add(option as u32 * 40503 + 1);
        let mut next = |bound: u32| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state % bound
        };
        let count = next(2) + 3;
        let mut text = String::new();
        for i in 0..count {
            if i > 0 {
                text.push(' ');
            }
            text.push_str(words[next(words.len() as u32) as usize]);
        }
        let style = crate::style::Style {
            font: FontId::ALT,
            ..Default::default()
        };
        let mut cells: Vec<Cell> = text.chars().map(|ch| Cell { ch, style }).collect();
        let mut width = 0.0;
        let mut keep = 0;
        for cell in &cells {
            width += cx.frame.ui().width(std::slice::from_ref(cell));
            if width > max_width as f32 {
                break;
            }
            keep += 1;
        }
        cells.truncate(keep);
        cells
    }

    fn draw_enchanting(&self, cx: &mut RenderCx<'_, '_>) {
        let view = cx.view;
        let (l, t) = (self.left, self.top);
        let (mx, my) = (self.mouse.0, self.mouse.1);
        let gold = view.slot(1).map_or(0, |s| s.count.max(0));
        for i in 0..3usize {
            let (x, y) = (l + 60, t + 14 + 19 * i as i32);
            let cost = view.data(i);
            if cost == 0 {
                cx.sprite(
                    "container/enchanting_table/enchantment_slot_disabled",
                    x,
                    y,
                    108,
                    19,
                );
                continue;
            }
            let cost_text = cost.to_string();
            let text_w = 86 - cx.text_width(&cost_text);
            let name = Self::galactic_name(view.data(3), i, text_w, cx);
            let level_sprite = |disabled: bool| {
                format!(
                    "container/enchanting_table/level_{}{}",
                    i + 1,
                    if disabled { "_disabled" } else { "" }
                )
            };
            let (name_color, cost_color);
            if (gold < i as i32 + 1 || view.xp_level() < cost) && !view.creative() {
                cx.sprite(
                    "container/enchanting_table/enchantment_slot_disabled",
                    x,
                    y,
                    108,
                    19,
                );
                cx.sprite(&level_sprite(true), x + 1, y + 1, 16, 16);
                name_color = 0xFF34_2F25;
                cost_color = 0xFF40_7F10;
            } else {
                let hovered = mx >= x as f32
                    && my >= y as f32
                    && mx < (x + 108) as f32
                    && my < (y + 19) as f32;
                let (bg, color) = if hovered {
                    (
                        "container/enchanting_table/enchantment_slot_highlighted",
                        0xFFFF_FF80,
                    )
                } else {
                    ("container/enchanting_table/enchantment_slot", 0xFF68_5E4A)
                };
                cx.sprite(bg, x, y, 108, 19);
                cx.sprite(&level_sprite(false), x + 1, y + 1, 16, 16);
                name_color = color;
                cost_color = 0xFF80_FF20;
            }
            cx.frame
                .cells(&name, (x + 20) as f32, (y + 2) as f32, name_color, false);
            let cost_w = cx.text_width(&cost_text);
            cx.text(&cost_text, x + 20 + 86 - cost_w, y + 9, cost_color, true);
            if mx >= x as f32 && my >= y as f32 && mx < (x + 108) as f32 && my < (y + 17) as f32 {
                self.enchant_tooltip(cx, i);
            }
        }
    }

    fn enchant_tooltip(&self, cx: &mut RenderCx<'_, '_>, i: usize) {
        let view = cx.view;
        let cost = view.data(i);
        let Some((key, level, max_level)) = view.enchant_clue(i) else {
            return;
        };
        if cost <= 0 || level < 0 {
            return;
        }
        let mut name = cx.lang(&key);
        if level != 1 || max_level != 1 {
            let numeral = cx.lang(&format!("enchantment.level.{level}"));
            name = format!("{name} {numeral}");
        }
        let mut lines = TooltipLines::default();
        lines.push_text(format!(
            "§f{}",
            cx.lang_fmt("container.enchant.clue", &[&name])
        ));
        if !view.creative() {
            lines.push_text("");
            let n = i as i32 + 1;
            if view.xp_level() < cost {
                lines.push_text(format!(
                    "§c{}",
                    cx.lang_fmt("container.enchant.level.requirement", &[&cost.to_string()])
                ));
            } else {
                let gold = view.slot(1).map_or(0, |s| s.count);
                let lapis = if n == 1 {
                    cx.lang("container.enchant.lapis.one")
                } else {
                    cx.lang_fmt("container.enchant.lapis.many", &[&n.to_string()])
                };
                lines.push_text(format!("{}{lapis}", if gold >= n { "§7" } else { "§c" }));
                let levels = if n == 1 {
                    cx.lang("container.enchant.level.one")
                } else {
                    cx.lang_fmt("container.enchant.level.many", &[&n.to_string()])
                };
                lines.push_text(format!("§7{levels}"));
            }
        }
        cx.set_tooltip(lines);
    }

    fn draw_anvil_cost(&self, cx: &mut RenderCx<'_, '_>) {
        let view = cx.view;
        let cost = view.data(0);
        if cost <= 0 {
            return;
        }
        let (text, color) = if cost >= 40 && !view.creative() {
            (cx.lang("container.repair.expensive"), 0xFFFF_6060)
        } else if is_empty(view.slot(2)) {
            return;
        } else {
            let affordable = view.creative() || view.xp_level() >= cost;
            (
                cx.lang_fmt("container.repair.cost", &[&cost.to_string()]),
                if affordable { 0xFF80_FF20 } else { 0xFFFF_6060 },
            )
        };
        let w = cx.text_width(&text);
        let tx = self.layout.width - 8 - w - 2;
        cx.frame.fill_rect(
            self.left + tx - 2,
            self.top + 67,
            self.left + self.layout.width - 8,
            self.top + 79,
            0x4F00_0000,
        );
        cx.text(&text, self.left + tx, self.top + 69, color, true);
    }

    fn draw_beacon(&self, cx: &mut RenderCx<'_, '_>) {
        let view = cx.view;
        let (l, t) = (self.left, self.top);
        for (name, x) in [
            ("netherite_ingot", 20),
            ("emerald", 41),
            ("diamond", 63),
            ("gold_ingot", 86),
            ("iron_ingot", 108),
        ] {
            if let Some(id) = mcr_data::v26_3().item_by_name(&format!("minecraft:{name}")) {
                let stack = ItemStack::new(id as i32, 1);
                cx.stack(&stack, l + x, t + 109);
            }
        }
        let primary = cx.lang("block.minecraft.beacon.primary");
        let secondary = cx.lang("block.minecraft.beacon.secondary");
        cx.centered_text(&primary, l + 62, t + 10, 0xFFE0_E0E0, true);
        cx.centered_text(&secondary, l + 169, t + 10, 0xFFE0_E0E0, true);
        let (mx, my) = self.mouse;
        let mut tip = None;
        for b in self.beacon.buttons.iter().map(|b| b.0) {
            if !self.beacon_visible(&b) {
                continue;
            }
            let active = self.beacon_active(view, &b);
            let hovered = contains(b.rect, mx, my);
            let sprite = if !active {
                "container/beacon/button_disabled"
            } else if self.beacon_selected(&b) {
                "container/beacon/button_selected"
            } else if hovered {
                "container/beacon/button_highlighted"
            } else {
                "container/beacon/button"
            };
            let [x, y, w, h] = b.rect;
            cx.sprite(sprite, x, y, w, h);
            let effect = match b.kind {
                BeaconKind::Upgrade => self.beacon.primary,
                _ => b.effect,
            };
            let icon = match (b.kind, effect) {
                (BeaconKind::Confirm, _) => Some("container/beacon/confirm".to_owned()),
                (BeaconKind::Cancel, _) => Some("container/beacon/cancel".to_owned()),
                (_, Some(e)) => Some(format!("mob_effect/{e}")),
                _ => None,
            };
            if let Some(icon) = icon {
                cx.sprite(&icon, x + 2, y + 2, 18, 18);
            }
            if hovered
                && let Some(e) = effect
                && !matches!(b.kind, BeaconKind::Confirm | BeaconKind::Cancel)
            {
                tip = Some(cx.lang(&format!("effect.minecraft.{e}")));
            }
        }
        if let Some(tip) = tip {
            cx.set_tooltip(TooltipLines::text(tip));
        }
    }

    fn draw_cartography(&self, cx: &mut RenderCx<'_, '_>) {
        let view = cx.view;
        let (l, t) = (self.left, self.top);
        let name_of = |s: Option<&ItemStack>| {
            s.and_then(StackView::new).map(|v| {
                v.name()
                    .strip_prefix("minecraft:")
                    .unwrap_or(v.name())
                    .to_owned()
            })
        };
        let extra = name_of(view.slot(1));
        let scaling = extra.as_deref() == Some("paper");
        let duplicating = extra.as_deref() == Some("map");
        let locking = extra.as_deref() == Some("glass_pane");
        if scaling {
            cx.sprite(
                "container/cartography_table/scaled_map",
                l + 67,
                t + 13,
                66,
                66,
            );
        } else if duplicating {
            cx.sprite(
                "container/cartography_table/duplicated_map",
                l + 67 + 16,
                t + 13,
                50,
                66,
            );
            cx.sprite(
                "container/cartography_table/duplicated_map",
                l + 67,
                t + 13 + 16,
                50,
                66,
            );
        } else {
            cx.sprite("container/cartography_table/map", l + 67, t + 13, 66, 66);
            if locking {
                cx.sprite(
                    "container/cartography_table/locked",
                    l + 118,
                    t + 60,
                    10,
                    14,
                );
            }
        }
    }

    fn draw_loom(&self, cx: &mut RenderCx<'_, '_>) {
        let view = cx.view;
        let (l, t) = (self.left, self.top);
        let scroll_y = (41.0 * self.list.offset) as i32;
        let active = self.scroll_extent(view).is_some();
        let sprite = if active {
            "container/loom/scroller"
        } else {
            "container/loom/scroller_disabled"
        };
        cx.sprite(sprite, l + 119, t + 13 + scroll_y, 12, 15);
        if self.loom_patterns_shown(view) {
            let count = view.loom_pattern_count().min(1024);
            let selected = view.data(0);
            let (mx, my) = self.mouse;
            for row in 0..4usize {
                for col in 0..4usize {
                    let index = (row + self.list.start) * 4 + col;
                    if index >= count {
                        return;
                    }
                    let rect = [l + 60 + col as i32 * 14, t + 13 + row as i32 * 14, 14, 14];
                    let sprite = if index as i32 == selected {
                        "container/loom/pattern_selected"
                    } else if contains(rect, mx, my) {
                        "container/loom/pattern_highlighted"
                    } else {
                        "container/loom/pattern"
                    };
                    cx.sprite(sprite, rect[0], rect[1], 14, 14);
                }
            }
        }
    }

    fn draw_stonecutter(&self, cx: &mut RenderCx<'_, '_>) {
        let view = cx.view;
        let (l, t) = (self.left, self.top);
        let options = view.stonecutter_options();
        let count = options.len().min(1024);
        let active = self.scroll_extent(view).is_some();
        let sy = (41.0 * self.list.offset) as i32;
        let sprite = if active {
            "container/stonecutter/scroller"
        } else {
            "container/stonecutter/scroller_disabled"
        };
        cx.sprite(sprite, l + 119, t + 15 + sy, 12, 15);
        let (mx, my) = self.mouse;
        let selected = view.data(0);
        for index in self.list.start..(self.list.start + 12).min(count) {
            let pos = index - self.list.start;
            let x = l + 52 + (pos % 4) as i32 * 16;
            let y = t + 14 + (pos / 4) as i32 * 18 + 2;
            let sprite = if index as i32 == selected {
                "container/stonecutter/recipe_selected"
            } else if mx >= x as f32
                && my >= y as f32
                && mx < (x + 16) as f32
                && my < (y + 18) as f32
            {
                "container/stonecutter/recipe_highlighted"
            } else {
                "container/stonecutter/recipe"
            };
            cx.sprite(sprite, x, y - 1, 16, 18);
        }
        let shown = (self.list.start + 12).min(count);
        for (index, option) in options.iter().enumerate().take(shown).skip(self.list.start) {
            let pos = index - self.list.start;
            let x = l + 52 + (pos % 4) as i32 * 16;
            let y = t + 14 + (pos / 4) as i32 * 18 + 2;
            cx.stack(option, x, y);
            if mx >= x as f32 && my >= y as f32 && mx < (x + 16) as f32 && my < (y + 18) as f32 {
                let lines = cx.item_tooltip(option);
                cx.set_tooltip(lines);
            }
        }
    }

    fn draw_merchant_background(&self, cx: &mut RenderCx<'_, '_>) {
        let (l, t) = (self.left, self.top);
        let Some(info) = cx.view.merchant() else {
            return;
        };
        if let Some(offer) = info.offers.get(self.merchant_selected)
            && offer.out_of_stock
        {
            cx.sprite(
                "container/villager/out_of_stock",
                l + 83 + 99,
                t + 35,
                28,
                21,
            );
        }
        let title_level =
            (info.level > 0 && info.level <= 5 && info.show_progress).then_some(info.level);
        let cells = cx.frame.ui().flatten(&self.title);
        let mut title: String = cells.iter().map(|c| c.ch).collect();
        if let Some(level) = title_level {
            let name = cx.lang(&format!("merchant.level.{level}"));
            title = cx.lang_fmt("merchant.title", &[&title, &name]);
        }
        let w = cx.text_width(&title);
        cx.text(
            &title,
            l + 49 + self.layout.width / 2 - w / 2,
            t + 6,
            LABEL,
            false,
        );
        let trades = cx.lang("merchant.trades");
        let w = cx.text_width(&trades);
        cx.text(&trades, l + 5 - w / 2 + 48, t + 6, LABEL, false);
    }

    fn draw_merchant_foreground(&mut self, cx: &mut RenderCx<'_, '_>) {
        let (l, t) = (self.left, self.top);
        let Some(info) = cx.view.merchant() else {
            return;
        };
        let offers = &info.offers[..info.offers.len().min(1024)];
        if offers.is_empty() {
            return;
        }
        let (mx, my) = self.mouse;
        let can_scroll = offers.len() > 7;
        let steps = offers.len() as i32 + 1 - 7;
        if steps > 1 {
            let left_over = 139 - (27 + (steps - 1) * 139 / steps);
            let step_height = 1 + left_over / steps + 139 / steps;
            let mut y = (113).min(self.merchant_scroll as i32 * step_height);
            if self.merchant_scroll as i32 == steps - 1 {
                y = 113;
            }
            cx.sprite("container/villager/scroller", l + 94, t + 18 + y, 6, 27);
        } else {
            cx.sprite(
                "container/villager/scroller_disabled",
                l + 94,
                t + 18,
                6,
                27,
            );
        }
        for (i, offer) in offers.iter().enumerate() {
            if can_scroll && !(self.merchant_scroll..self.merchant_scroll + 7).contains(&i) {
                continue;
            }
            let row = i - if can_scroll { self.merchant_scroll } else { 0 };
            let rect = [l + 5, t + 18 + 20 * row as i32, 88, 20];
            let hovered = contains(rect, mx, my);
            let sprite = if hovered || i == self.merchant_selected {
                "widget/button_highlighted"
            } else {
                "widget/button"
            };
            cx.sprite(sprite, rect[0], rect[1], 88, 20);
            let deco = t + 17 + 20 * row as i32 + 2;
            let x1 = l + 10;
            if offer.base_cost_a.count == offer.cost_a.count {
                cx.stack(&offer.cost_a, x1, deco);
            } else {
                let mut icon = offer.cost_a.clone();
                icon.count = 1;
                cx.stack(&icon, x1, deco);
                self.count_text(cx, offer.base_cost_a.count, x1, deco);
                self.count_text(cx, offer.cost_a.count, x1 + 14, deco);
                cx.sprite(
                    "container/villager/discount_strikethrough",
                    x1 + 7,
                    deco + 12,
                    9,
                    2,
                );
            }
            if !offer.cost_b.is_empty() {
                cx.stack(&offer.cost_b, l + 40, deco);
            }
            let arrow = if offer.out_of_stock {
                "container/villager/trade_arrow_out_of_stock"
            } else {
                "container/villager/trade_arrow"
            };
            cx.sprite(arrow, l + 60, deco + 3, 10, 9);
            cx.stack(&offer.result, l + 73, deco);
            if hovered {
                let part = if mx < (rect[0] + 20) as f32 {
                    Some(&offer.cost_a)
                } else if mx < (rect[0] + 50) as f32 && mx > (rect[0] + 30) as f32 {
                    Some(&offer.cost_b).filter(|s| !s.is_empty())
                } else if mx > (rect[0] + 65) as f32 {
                    Some(&offer.result)
                } else {
                    None
                };
                if let Some(stack) = part {
                    let lines = cx.item_tooltip(stack);
                    cx.set_tooltip(lines);
                }
            }
        }
        if let Some(offer) = offers.get(self.merchant_selected) {
            if info.show_progress && info.level < 5 {
                self.draw_trader_progress(cx, info.level, info.xp, info.future_xp);
            }
            if offer.out_of_stock && info.can_restock && self.inside([186, 35, 22, 21], mx, my) {
                let text = cx.lang("merchant.deprecated");
                cx.set_tooltip(TooltipLines::text(text));
            }
        }
    }

    fn count_text(&self, cx: &mut RenderCx<'_, '_>, count: i32, x: i32, y: i32) {
        let text = count.to_string();
        let w = cx.text_width(&text);
        cx.text(&text, x + 19 - 2 - w, y + 6 + 3, 0xFFFF_FFFF, true);
    }

    fn draw_trader_progress(&self, cx: &mut RenderCx<'_, '_>, level: i32, xp: i32, future: i32) {
        let (l, t) = (self.left, self.top);
        cx.sprite(
            "container/villager/experience_bar_background",
            l + 136,
            t + 16,
            102,
            5,
        );
        if !(1..5).contains(&level) {
            return;
        }
        let min = EXPERIENCE_THRESHOLDS[(level - 1) as usize];
        let max = EXPERIENCE_THRESHOLDS[level as usize];
        if xp >= min && max > min {
            let mult = 102.0 / (max - min) as f32;
            let w = ((mult * (i64::from(xp) - i64::from(min)) as f32).floor() as i32).min(102);
            self.region(
                cx,
                "container/villager/experience_bar_current",
                (0, 0),
                (l + 136, t + 16, w, 5),
            );
            if future > 0 {
                let fw = ((future as f32 * mult).floor() as i32).min(102 - w);
                self.region(
                    cx,
                    "container/villager/experience_bar_result",
                    (w, 0),
                    (l + 136 + w, t + 16, fw, 5),
                );
            }
        }
    }

    pub(super) fn render_book(&mut self, cx: &mut RenderCx<'_, '_>) {
        let left = (self.gui.0 - 192) / 2;
        cx.texture("book", (0, 0), (left, 2, 192, 192));
        let view = cx.view;
        let pages = &view.book_pages();
        let page = view.data(0).max(0) as usize;
        let total = pages.len().max(1);
        let indicator = cx.lang_fmt(
            "book.pageIndicator",
            &[&(page + 1).to_string(), &total.to_string()],
        );
        let w = cx.text_width(&indicator);
        cx.text(&indicator, left + 148 - w, 2 + 16, 0xFF00_0000, false);
        if let Some(component) = pages.get(page) {
            if self
                .book_layout
                .as_ref()
                .is_none_or(|l| !l.is_for(component, page))
            {
                self.book_layout = Some(PageLayout::new(cx, component, page));
            }
        } else {
            self.book_layout = None;
        }
        if let Some(layout) = &self.book_layout {
            layout.draw(cx, self.gui.0);
        }
        if let Some(b) = self.widgets.button_mut(BOOK_PREV) {
            b.visible = page > 0;
        }
        if let Some(b) = self.widgets.button_mut(BOOK_NEXT) {
            b.visible = page + 1 < pages.len();
        }
        self.widgets.render(cx);
        for (id, forward) in [(BOOK_PREV, false), (BOOK_NEXT, true)] {
            if let Some(b) = self.widgets.button_mut(id)
                && b.visible
            {
                let [x, y, ..] = b.rect;
                let hovered = contains(b.rect, cx.mouse.0, cx.mouse.1);
                let name = format!(
                    "widget/page_{}{}",
                    if forward { "forward" } else { "backward" },
                    if hovered { "_highlighted" } else { "" }
                );
                cx.sprite(&name, x, y, 23, 13);
            }
        }
    }

    pub(super) fn book_input(
        &mut self,
        env: &mut Env<'_>,
        input: &UiInput,
        out: &mut Vec<ScreenAction>,
    ) -> bool {
        if let UiInput::Key { key, .. } = *input {
            match key {
                Key::Escape => {
                    out.push(ScreenAction::CloseContainer);
                    return true;
                }
                Key::Left => {
                    out.push(ScreenAction::ContainerButton(1));
                    return true;
                }
                Key::Right => {
                    out.push(ScreenAction::ContainerButton(2));
                    return true;
                }
                _ => {}
            }
        }
        if let UiInput::MousePress { x, y, .. } = *input
            && let Some(event) = self
                .book_layout
                .as_ref()
                .and_then(|l| l.event_at(self.gui.0, x, y))
        {
            match event {
                ClickEvent::ChangePage(n) => {
                    let page = n.saturating_sub(1).max(0);
                    out.push(ScreenAction::ContainerButton(100 + page));
                }
                other => click_event_actions(other, out),
            }
            return true;
        }
        let mut events = Vec::new();
        let used = self.widgets.input(env, input, &mut events);
        self.widget_events(&events, out);
        used
    }
}

fn fraction(value: i32, total: i32) -> f32 {
    if total <= 0 || value <= 0 {
        return 0.0;
    }
    (value as f32 / total as f32).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use mcr_protocol::types::component::DataComponent;
    use mcr_protocol::types::text::Component;

    use super::*;

    fn anvil(item: ItemStack, hover: &str) -> ContainerScreen {
        let mut screen = ContainerScreen::new(MenuKind::Anvil, Component::text(""));
        screen.anvil_item = Some(item);
        screen.anvil_hover = hover.to_owned();
        screen
    }

    #[test]
    fn placing_an_unnamed_item_syncs_an_empty_rename() {
        let mut screen = anvil(ItemStack::new(1, 1), "Iron Sword");
        screen.send_rename("Iron Sword");
        assert_eq!(screen.pending_rename.take().as_deref(), Some(""));
        screen.send_rename("Iron Sword");
        assert_eq!(screen.pending_rename, None);
    }

    #[test]
    fn placing_a_named_item_syncs_its_name_and_edits_follow() {
        let mut item = ItemStack::new(1, 1);
        item.components
            .added
            .push(DataComponent::CustomName(Component::text("Excalibur")));
        let mut screen = anvil(item, "Excalibur");
        screen.send_rename("Excalibur");
        assert_eq!(screen.pending_rename.take().as_deref(), Some("Excalibur"));
        screen.send_rename("Excalibur II");
        assert_eq!(
            screen.pending_rename.take().as_deref(),
            Some("Excalibur II")
        );
    }

    #[test]
    fn progress_survives_hostile_totals() {
        assert_eq!(fraction(5, 0), 0.0);
        assert_eq!(fraction(5, -3), 0.0);
        assert_eq!(fraction(-5, 10), 0.0);
        assert_eq!(fraction(i32::MAX, 1), 1.0);
        assert_eq!(fraction(i32::MIN, i32::MIN), 0.0);
        assert_eq!(fraction(1, 2), 0.5);
    }
}
