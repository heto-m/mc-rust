use mcr_protocol::types::item::ItemStack;
use mcr_protocol::types::text::Component;

use super::click::{ClickMachine, SlotGeometry, can_quick_replace, quick_craft_place_count};
use super::kind::{ContainerLayout, MenuKind, SlotKind};
use crate::gui::env::{Env, GameView, RenderCx, is_empty};
use crate::gui::input::{Key, MouseButton, UiInput, contains};
use crate::gui::recipe_book::RecipeBook;
use crate::gui::screen::{ClickType, ContainerClick, Screen, ScreenAction};
use crate::gui::widgets::WidgetSet;
use crate::items::StackView;
use mcr_assets::ItemProps;

pub(super) const LABEL: u32 = 0xFF40_4040;
const OVERLAY_TOP: u32 = 0xC010_1010;
const OVERLAY_BOTTOM: u32 = 0xD010_1010;
const QUICK_PREVIEW: u32 = 0x80FF_FFFF;

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ScrollState {
    pub offset: f32,
    pub start: usize,
    pub dragging: bool,
}

#[derive(Debug)]
pub struct ContainerScreen {
    pub(super) layout: ContainerLayout,
    pub(super) title: Component,
    pub(super) left: i32,
    pub(super) top: i32,
    pub(super) gui: (i32, i32),
    pub(super) machine: ClickMachine,
    pub(super) widgets: WidgetSet,
    pub(super) mouse: (f32, f32),
    pub(super) list: ScrollState,
    pub(super) merchant_selected: usize,
    pub(super) merchant_scroll: usize,
    pub(super) beacon: super::menus::BeaconState,
    pub(super) crafter_override: [Option<bool>; 9],
    pub(super) anvil_item: Option<ItemStack>,
    pub(super) anvil_sent: Option<String>,
    pub(super) anvil_hover: String,
    pub(super) pending_rename: Option<String>,
    pub(super) book_layout: Option<crate::gui::book::PageLayout>,
    pub(super) recipes: Option<RecipeBook>,
}

impl ContainerScreen {
    pub fn new(kind: MenuKind, title: Component) -> Self {
        Self {
            layout: ContainerLayout::new(kind),
            title,
            left: 0,
            top: 0,
            gui: (0, 0),
            machine: ClickMachine::default(),
            widgets: WidgetSet::default(),
            mouse: (0.0, 0.0),
            list: ScrollState::default(),
            merchant_selected: 0,
            merchant_scroll: 0,
            beacon: Default::default(),
            crafter_override: [None; 9],
            anvil_item: None,
            anvil_sent: None,
            anvil_hover: String::new(),
            pending_rename: None,
            book_layout: None,
            recipes: RecipeBook::for_menu(kind),
        }
    }

    pub fn player_inventory() -> Self {
        Self::new(
            MenuKind::PlayerInventory,
            Component::translatable("container.crafting", Vec::new()),
        )
    }

    pub fn kind(&self) -> MenuKind {
        self.layout.kind
    }

    pub fn layout(&self) -> &ContainerLayout {
        &self.layout
    }

    pub fn origin(&self) -> (i32, i32) {
        (self.left, self.top)
    }

    pub(super) fn geometry(&self) -> SlotGeometry<'_> {
        SlotGeometry {
            left: self.left,
            top: self.top,
            width: self.layout.width,
            height: self.layout.height,
            slots: &self.layout.slots,
        }
    }

    pub fn hovered_slot(&self) -> Option<usize> {
        self.geometry().slot_at(self.mouse.0, self.mouse.1)
    }

    pub(super) fn inside(&self, rect: [i32; 4], x: f32, y: f32) -> bool {
        crate::gui::input::contains(
            [self.left + rect[0], self.top + rect[1], rect[2], rect[3]],
            x,
            y,
        )
    }

    fn quick_craft_remaining(&self, view: &dyn GameView) -> Option<i32> {
        let carried = view.carried().filter(|c| !c.is_empty())?;
        if !self.machine.quick_crafting() {
            return None;
        }
        let max = max_stack(carried);
        if self.machine.quick_kind() == 2 {
            return Some(max);
        }
        let slots = self.machine.quick_slots();
        let place = quick_craft_place_count(slots.len(), self.machine.quick_kind(), carried);
        let mut remaining = i64::from(carried.count);
        for &s in slots {
            let carry = view
                .slot(s)
                .filter(|x| !x.is_empty())
                .map_or(0, |x| x.count);
            let new = place.saturating_add(carry).min(max);
            remaining -= i64::from(new) - i64::from(carry);
        }
        Some(remaining.clamp(0, i64::from(i32::MAX)) as i32)
    }

    fn draw_slots(&mut self, cx: &mut RenderCx<'_, '_>) {
        let view = cx.view;
        let drag_slots = self.machine.quick_slots().to_vec();
        let carried = view.carried().filter(|c| !c.is_empty());
        let hovered = self.hovered_slot();
        if let Some(h) = hovered {
            let s = self.layout.slots[h];
            cx.sprite(
                "container/slot_highlight_back",
                self.left + s.x - 4,
                self.top + s.y - 4,
                24,
                24,
            );
        }
        for (i, spec) in self.layout.slots.iter().enumerate() {
            if !spec.active {
                continue;
            }
            let (x, y) = (self.left + spec.x, self.top + spec.y);
            if spec.kind == SlotKind::Crafter {
                let disabled = self
                    .crafter_override
                    .get(i)
                    .copied()
                    .flatten()
                    .unwrap_or(view.data(i) == 1);
                if disabled {
                    cx.sprite("container/crafter/disabled_slot", x - 1, y - 1, 18, 18);
                    continue;
                }
            }
            let mut stack = view.slot(i).filter(|s| !s.is_empty()).cloned();
            let mut preview = false;
            if let (true, Some(carried)) = (drag_slots.contains(&i), carried) {
                if drag_slots.len() == 1 {
                    continue;
                }
                if can_quick_replace(view.slot(i), carried) {
                    preview = true;
                    let max = max_stack(carried);
                    let carry = stack.as_ref().map_or(0, |s| s.count);
                    let place = quick_craft_place_count(
                        drag_slots.len(),
                        self.machine.quick_kind(),
                        carried,
                    );
                    let mut shown = carried.clone();
                    shown.count = place.saturating_add(carry).min(max);
                    stack = Some(shown);
                }
            }
            match stack {
                None => {
                    if let Some(icon) = spec.icon {
                        cx.sprite(icon, x, y, 16, 16);
                    }
                }
                Some(stack) => {
                    if preview {
                        cx.frame.fill_rect(x, y, x + 16, y + 16, QUICK_PREVIEW);
                    }
                    cx.stack(&stack, x, y);
                }
            }
        }
        if let Some(h) = hovered {
            let s = self.layout.slots[h];
            cx.sprite(
                "container/slot_highlight_front",
                self.left + s.x - 4,
                self.top + s.y - 4,
                24,
                24,
            );
        }
    }

    fn draw_carried(&mut self, cx: &mut RenderCx<'_, '_>) {
        let Some(carried) = cx.view.carried().filter(|c| !c.is_empty()) else {
            return;
        };
        let (mx, my) = (self.mouse.0 as i32, self.mouse.1 as i32);
        let mut shown = carried.clone();
        let mut zero = false;
        if self.machine.quick_slots().len() > 1
            && let Some(rest) = self.quick_craft_remaining(cx.view)
        {
            shown.count = rest.max(1);
            zero = rest <= 0;
        }
        cx.stack(&shown, mx - 8, my - 8);
        if zero {
            cx.text("0", mx - 8 + 19 - 2 - 6, my - 8 + 6 + 3, 0xFFFF_FF55, true);
        }
    }

    fn draw_labels(&mut self, cx: &mut RenderCx<'_, '_>) {
        let (title_x, title_y) = self.layout.title_pos;
        if title_x > -500 {
            let mut x = self.left + title_x;
            if self.layout.title_centered {
                let w = cx.frame.ui().flatten(&self.title);
                let w = cx.frame.ui().width(&w) as i32;
                x = self.left + (self.layout.width - w) / 2;
            }
            cx.frame.component(
                &self.title,
                x as f32,
                (self.top + title_y) as f32,
                LABEL,
                false,
            );
        }
        if let Some((x, y)) = self.layout.inventory_label {
            let text = cx.lang("container.inventory");
            cx.text(&text, self.left + x, self.top + y, LABEL, false);
        }
    }
}

fn geometry_of(layout: &ContainerLayout, left: i32, top: i32) -> SlotGeometry<'_> {
    SlotGeometry {
        left,
        top,
        width: layout.width,
        height: layout.height,
        slots: &layout.slots,
    }
}

pub(super) fn max_stack(stack: &ItemStack) -> i32 {
    StackView::new(stack).map_or(64, |v| v.max_stack_size())
}

impl Screen for ContainerScreen {
    fn init(&mut self, width: i32, height: i32) {
        self.gui = (width, height);
        if let Some(book) = &mut self.recipes {
            book.resize((width, height));
        }
        self.left = self.wanted_left();
        self.top = (height - self.layout.height) / 2;
        self.setup_widgets();
    }

    fn tick(&mut self, env: &mut Env<'_>, out: &mut Vec<ScreenAction>) {
        if let Some(name) = self.pending_rename.take() {
            out.push(ScreenAction::RenameItem(name));
        }
        if let Some(book) = &mut self.recipes {
            book.tick(env.view, &self.layout);
            book.take_actions(out);
        }
    }

    fn slot_center(&self, index: usize) -> Option<(i32, i32)> {
        let s = self.layout.slots.get(index)?;
        Some((self.left + s.x + 8, self.top + s.y + 8))
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        self.mouse = cx.mouse;
        let (gw, gh) = cx.gui_size();
        cx.frame
            .fill_gradient(0, 0, gw, gh, OVERLAY_TOP, OVERLAY_BOTTOM);
        if self.layout.kind == MenuKind::Lectern {
            self.render_book(cx);
            return;
        }
        self.sync_left();
        if self
            .recipes
            .as_ref()
            .is_some_and(RecipeBook::is_narrow_open)
        {
            self.render_recipes(cx);
            return;
        }
        self.sync(cx);
        self.draw_background(cx);
        self.widgets.render(cx);
        self.draw_labels(cx);
        self.draw_slots(cx);
        self.draw_ghost(cx);
        self.draw_foreground(cx);
        self.render_recipes(cx);
        self.draw_carried(cx);
        if is_empty(cx.view.carried())
            && let Some(h) = self.hovered_slot()
        {
            if let Some(stack) = cx.view.slot(h).filter(|s| !s.is_empty()) {
                let lines = cx.item_tooltip(stack);
                cx.set_tooltip(lines);
            } else if let Some(book) = &self.recipes {
                book.ghost_tooltip(cx, h);
            }
        }
    }

    fn input(&mut self, env: &mut Env<'_>, input: &UiInput, out: &mut Vec<ScreenAction>) -> bool {
        if self.layout.kind == MenuKind::Lectern {
            return self.book_input(env, input, out);
        }
        let view = env.view;
        if self.recipe_input(env, input, out) {
            return true;
        }
        match *input {
            UiInput::MousePress {
                x,
                y,
                button,
                mods,
                double,
            } => {
                if self.pre_press(env, (x, y), button, out) {
                    return true;
                }
                let mut clicks = Vec::new();
                let g = geometry_of(&self.layout, self.left, self.top);
                self.machine
                    .press(&g, view, (x, y), button, mods, double, &mut clicks);
                self.emit(view, clicks, out);
                true
            }
            UiInput::MouseMove { x, y } => {
                self.mouse = (x, y);
                self.drag_scroll(view, y);
                let g = geometry_of(&self.layout, self.left, self.top);
                self.machine.mouse_move(&g, view, (x, y));
                let mut events = Vec::new();
                self.widgets.input(env, input, &mut events);
                true
            }
            UiInput::MouseRelease { x, y, button, mods } => {
                self.list.dragging = false;
                let mut clicks = Vec::new();
                let g = geometry_of(&self.layout, self.left, self.top);
                self.machine
                    .release(&g, view, (x, y), button, mods, &mut clicks);
                self.emit(view, clicks, out);
                true
            }
            UiInput::Scroll { dy, .. } => self.scroll(view, dy),
            UiInput::Key { key, mods, .. } => {
                if key == Key::Escape {
                    out.push(ScreenAction::CloseContainer);
                    return true;
                }
                let mut events = Vec::new();
                if self.widgets.input(env, input, &mut events) {
                    self.widget_events(&events, out);
                    return true;
                }
                if self.text_focused() {
                    return true;
                }
                if key == Key::Letter('e') {
                    out.push(ScreenAction::CloseContainer);
                    return true;
                }
                let mut clicks = Vec::new();
                let g = geometry_of(&self.layout, self.left, self.top);
                let handled = self
                    .machine
                    .key(&g, view, self.mouse, key, mods, &mut clicks);
                self.emit(view, clicks, out);
                handled
            }
            UiInput::Char(_) => {
                let mut events = Vec::new();
                let used = self.widgets.input(env, input, &mut events);
                self.widget_events(&events, out);
                used
            }
        }
    }

    fn entity_rect(&self) -> Option<[i32; 4]> {
        let (x, y, w, h) = match self.layout.kind {
            MenuKind::PlayerInventory => (26, 8, 49, 70),
            MenuKind::Mount { .. } => (26, 18, 52, 52),
            MenuKind::Smithing => (121, 20, 40, 60),
            MenuKind::Enchantment => (14, 14, 38, 31),
            _ => return None,
        };
        Some([self.left + x, self.top + y, w, h])
    }
}

impl ContainerScreen {
    /// Sends clicks to the app, first toggling crafter slots the way vanilla's `CrafterScreen` does.
    fn emit(
        &mut self,
        view: &dyn GameView,
        clicks: Vec<ContainerClick>,
        out: &mut Vec<ScreenAction>,
    ) {
        for click in clicks {
            if let (Some(book), Ok(slot)) = (&mut self.recipes, usize::try_from(click.slot)) {
                book.crafting_slot_clicked(slot, &self.layout);
            }
            self.crafter_hook(view, click, out);
            out.push(ScreenAction::ContainerClick(click));
        }
    }

    fn crafter_hook(
        &mut self,
        view: &dyn GameView,
        click: ContainerClick,
        out: &mut Vec<ScreenAction>,
    ) {
        if self.layout.kind != MenuKind::Crafter || view.spectator() {
            return;
        }
        let Ok(slot) = usize::try_from(click.slot) else {
            return;
        };
        if slot >= 9 || !is_empty(view.slot(slot)) {
            return;
        }
        let disabled = self.crafter_override[slot].unwrap_or(view.data(slot) == 1);
        let toggle = match click.mode {
            ClickType::Pickup if disabled => Some(true),
            ClickType::Pickup if is_empty(view.carried()) => Some(false),
            ClickType::Swap if disabled => {
                let swapped = if click.button == 40 {
                    view.offhand()
                } else {
                    let button = usize::try_from(click.button).unwrap_or(usize::MAX);
                    let start = self.layout.player_start() + 27;
                    start.checked_add(button).and_then(|i| view.slot(i))
                };
                (!is_empty(swapped)).then_some(true)
            }
            _ => None,
        };
        if let Some(enabled) = toggle {
            self.crafter_override[slot] = Some(!enabled);
            out.push(ScreenAction::CrafterSlotState { slot, enabled });
            out.push(ScreenAction::ClickSound);
        }
    }

    fn wanted_left(&self) -> i32 {
        match &self.recipes {
            Some(book) => book.container_left(self.layout.width),
            None => (self.gui.0 - self.layout.width) / 2,
        }
    }

    fn sync_left(&mut self) {
        let left = self.wanted_left();
        if left != self.left {
            self.left = left;
            self.setup_widgets();
        }
    }

    fn render_recipes(&mut self, cx: &mut RenderCx<'_, '_>) {
        if let Some(book) = &mut self.recipes {
            book.render(cx, self.left);
        }
    }

    fn draw_ghost(&mut self, cx: &mut RenderCx<'_, '_>) {
        let big = self.layout.kind != MenuKind::PlayerInventory;
        if let Some(book) = &mut self.recipes {
            book.render_ghost(cx, &self.layout, (self.left, self.top), big);
        }
    }

    /// Lets the recipe book take the input first; true when it consumed it.
    fn recipe_input(
        &mut self,
        env: &mut Env<'_>,
        input: &UiInput,
        out: &mut Vec<ScreenAction>,
    ) -> bool {
        let Some(book) = &mut self.recipes else {
            return false;
        };
        if let UiInput::MousePress {
            x,
            y,
            button: MouseButton::Left,
            ..
        } = *input
            && contains(book.button_rect(self.left), x, y)
            && !env.view.spectator()
        {
            book.toggle();
            book.take_actions(out);
            out.push(ScreenAction::ClickSound);
            self.sync_left();
            return true;
        }
        let used = book.input(env, input, out);
        self.sync_left();
        if used {
            return true;
        }
        let Some(book) = &self.recipes else {
            return false;
        };
        match *input {
            UiInput::MousePress { x, y, .. } if book.is_open() => {
                let outside_beside =
                    contains([self.left - 147, self.top, 147, self.layout.height], x, y);
                book.is_narrow_open() || outside_beside
            }
            _ => false,
        }
    }

    pub(super) fn text_focused(&self) -> bool {
        self.recipes
            .as_ref()
            .is_some_and(RecipeBook::capturing_text)
            || self
                .widgets
                .focused()
                .and_then(|id| self.widgets.edit(id))
                .is_some_and(|e| e.active && e.editable)
    }
}

#[allow(dead_code)]
fn unused(_: MouseButton) {}
