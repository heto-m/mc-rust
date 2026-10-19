use mcr_assets::ItemProps;
use mcr_protocol::types::item::ItemStack;

use super::kind::{SlotKind, SlotSpec};
use crate::gui::env::{GameView, is_empty};
use crate::gui::input::{Key, Modifiers, MouseButton};
use crate::gui::screen::{ClickType, ContainerClick};
use crate::items::StackView;

#[derive(Clone, Copy, Debug)]
pub struct SlotGeometry<'a> {
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
    pub slots: &'a [SlotSpec],
}

impl SlotGeometry<'_> {
    pub fn slot_at(&self, x: f32, y: f32) -> Option<usize> {
        let (xm, ym) = (x - self.left as f32, y - self.top as f32);
        self.slots.iter().position(|s| {
            s.active
                && xm >= (s.x - 1) as f32
                && xm < (s.x + 17) as f32
                && ym >= (s.y - 1) as f32
                && ym < (s.y + 17) as f32
        })
    }

    pub fn outside(&self, x: f32, y: f32) -> bool {
        x < self.left as f32
            || y < self.top as f32
            || x >= (self.left + self.width) as f32
            || y >= (self.top + self.height) as f32
    }
}

fn item_name(stack: &ItemStack) -> Option<String> {
    StackView::new(stack).map(|v| v.name().to_owned())
}

fn max_stack(stack: &ItemStack) -> i32 {
    StackView::new(stack).map_or(64, |v| v.max_stack_size())
}

pub fn may_place(spec: &SlotSpec, stack: &ItemStack) -> bool {
    let name = || {
        item_name(stack)
            .map(|n| n.strip_prefix("minecraft:").unwrap_or(&n).to_owned())
            .unwrap_or_default()
    };
    match spec.kind {
        SlotKind::Result | SlotKind::Locked => false,
        SlotKind::Armor(part) => {
            let n = name();
            match part {
                0 => {
                    n.ends_with("_helmet")
                        || n == "carved_pumpkin"
                        || n.ends_with("_head")
                        || n.ends_with("_skull")
                }
                1 => n.ends_with("_chestplate") || n == "elytra",
                2 => n.ends_with("_leggings"),
                _ => n.ends_with("_boots"),
            }
        }
        SlotKind::Saddle => name() == "saddle",
        SlotKind::MountArmor => {
            let n = name();
            n.ends_with("_horse_armor") || n.ends_with("_carpet") || n.ends_with("nautilus_armor")
        }
        _ => true,
    }
}

/// Port of vanilla `canItemQuickReplace(slot, carried, ignoreSize = true)`.
pub fn can_quick_replace(slot: Option<&ItemStack>, carried: &ItemStack) -> bool {
    let empty = is_empty(slot);
    match slot {
        Some(s) if !empty && s.item == carried.item && s.components == carried.components => {
            s.count <= max_stack(carried)
        }
        _ => empty,
    }
}

pub fn quick_craft_mask(header: i32, kind: i32) -> i32 {
    header & 3 | (kind & 3) << 2
}

pub fn quick_craft_place_count(slots: usize, kind: i32, carried: &ItemStack) -> i32 {
    match kind {
        0 => (carried.count as f32 / slots.max(1) as f32).floor() as i32,
        1 => 1,
        2 => max_stack(carried),
        _ => carried.count,
    }
}

#[derive(Clone, Debug, Default)]
pub struct ClickMachine {
    quick_crafting: bool,
    qc_button: Option<MouseButton>,
    qc_kind: i32,
    qc_slots: Vec<usize>,
    skip_release: bool,
    double: bool,
    last_click_slot: Option<usize>,
    last_quick_moved: Option<ItemStack>,
    pressed: bool,
}

fn send(
    out: &mut Vec<ContainerClick>,
    slot: Option<usize>,
    slot_id: i32,
    button: i32,
    mode: ClickType,
) {
    let slot = slot.map_or(slot_id, |s| s as i32);
    out.push(ContainerClick::new(slot, button, mode));
}

fn kept(stack: Option<&ItemStack>) -> Option<ItemStack> {
    stack.filter(|s| !s.is_empty()).cloned()
}

impl ClickMachine {
    pub fn quick_crafting(&self) -> bool {
        self.quick_crafting
    }

    pub fn quick_slots(&self) -> &[usize] {
        &self.qc_slots
    }

    pub fn quick_kind(&self) -> i32 {
        self.qc_kind
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    #[allow(clippy::too_many_arguments)]
    pub fn press(
        &mut self,
        g: &SlotGeometry<'_>,
        view: &dyn GameView,
        (x, y): (f32, f32),
        button: MouseButton,
        mods: Modifiers,
        double: bool,
        out: &mut Vec<ContainerClick>,
    ) {
        let slot = g.slot_at(x, y);
        self.double = self.last_click_slot == slot && double;
        self.skip_release = false;
        self.pressed = true;
        let cloning = button == MouseButton::Middle && view.creative();
        if matches!(button, MouseButton::Left | MouseButton::Right) || cloning {
            let mut slot_id = slot.map_or(-1, |s| s as i32);
            if g.outside(x, y) {
                slot_id = ContainerClick::OUTSIDE;
            }
            if slot_id != -1 && !self.quick_crafting {
                if is_empty(view.carried()) {
                    if cloning {
                        send(out, slot, slot_id, button.index(), ClickType::Clone);
                    } else {
                        let quick = slot_id != ContainerClick::OUTSIDE && mods.shift;
                        let mut mode = ClickType::Pickup;
                        if quick {
                            self.last_quick_moved = slot.and_then(|s| kept(view.slot(s)));
                            mode = ClickType::QuickMove;
                        } else if slot_id == ContainerClick::OUTSIDE {
                            mode = ClickType::Throw;
                        }
                        send(out, slot, slot_id, button.index(), mode);
                    }
                    self.skip_release = true;
                } else {
                    self.quick_crafting = true;
                    self.qc_button = Some(button);
                    self.qc_slots.clear();
                    self.qc_kind = match button {
                        MouseButton::Left => 0,
                        MouseButton::Right => 1,
                        _ => 2,
                    };
                }
            }
        }
        self.last_click_slot = slot;
    }

    fn should_add(&self, g: &SlotGeometry<'_>, view: &dyn GameView, slot: usize) -> bool {
        let Some(carried) = view.carried().filter(|c| !c.is_empty()) else {
            return false;
        };
        self.quick_crafting
            && (carried.count as usize > self.qc_slots.len() || self.qc_kind == 2)
            && can_quick_replace(view.slot(slot), carried)
            && may_place(&g.slots[slot], carried)
    }

    pub fn mouse_move(&mut self, g: &SlotGeometry<'_>, view: &dyn GameView, (x, y): (f32, f32)) {
        if !self.pressed || !self.quick_crafting {
            return;
        }
        if let Some(slot) = g.slot_at(x, y)
            && self.should_add(g, view, slot)
            && !self.qc_slots.contains(&slot)
        {
            self.qc_slots.push(slot);
        }
    }

    pub fn release(
        &mut self,
        g: &SlotGeometry<'_>,
        view: &dyn GameView,
        (x, y): (f32, f32),
        button: MouseButton,
        mods: Modifiers,
        out: &mut Vec<ContainerClick>,
    ) {
        self.pressed = false;
        let slot = g.slot_at(x, y);
        let mut slot_id = slot.map_or(-1, |s| s as i32);
        if g.outside(x, y) {
            slot_id = ContainerClick::OUTSIDE;
        }
        if self.double
            && button == MouseButton::Left
            && slot.is_some_and(|s| g.slots[s].takes_pick_all())
        {
            let slot_index = slot.unwrap_or_default();
            if mods.shift {
                if let Some(moved) = self.last_quick_moved.clone() {
                    let group = g.slots[slot_index].group;
                    for (i, target) in g.slots.iter().enumerate() {
                        if target.may_pickup()
                            && !is_empty(view.slot(i))
                            && target.group == group
                            && can_quick_replace(view.slot(i), &moved)
                        {
                            send(out, Some(i), i as i32, 0, ClickType::QuickMove);
                        }
                    }
                }
            } else {
                send(out, slot, slot_id, 0, ClickType::PickupAll);
            }
            self.double = false;
        } else {
            if self.quick_crafting && self.qc_button != Some(button) {
                self.quick_crafting = false;
                self.qc_slots.clear();
                self.skip_release = true;
                return;
            }
            if self.skip_release {
                self.skip_release = false;
                return;
            }
            if self.quick_crafting && !self.qc_slots.is_empty() {
                self.finish_quick_craft(out);
            } else if !is_empty(view.carried()) {
                if button == MouseButton::Middle {
                    send(out, slot, slot_id, button.index(), ClickType::Clone);
                } else {
                    let quick = slot_id != ContainerClick::OUTSIDE && mods.shift;
                    if quick {
                        self.last_quick_moved = slot.and_then(|s| kept(view.slot(s)));
                    }
                    let mode = if quick {
                        ClickType::QuickMove
                    } else {
                        ClickType::Pickup
                    };
                    send(out, slot, slot_id, button.index(), mode);
                }
            }
        }
        self.quick_crafting = false;
    }

    fn finish_quick_craft(&mut self, out: &mut Vec<ContainerClick>) {
        let kind = self.qc_kind;
        send(
            out,
            None,
            ContainerClick::OUTSIDE,
            quick_craft_mask(0, kind),
            ClickType::QuickCraft,
        );
        for &slot in &self.qc_slots {
            send(
                out,
                Some(slot),
                slot as i32,
                quick_craft_mask(1, kind),
                ClickType::QuickCraft,
            );
        }
        send(
            out,
            None,
            ContainerClick::OUTSIDE,
            quick_craft_mask(2, kind),
            ClickType::QuickCraft,
        );
    }

    pub fn key(
        &mut self,
        g: &SlotGeometry<'_>,
        view: &dyn GameView,
        mouse: (f32, f32),
        key: Key,
        mods: Modifiers,
        out: &mut Vec<ContainerClick>,
    ) -> bool {
        let Some(hovered) = g.slot_at(mouse.0, mouse.1) else {
            return false;
        };
        let keys = view.screen_keys();
        if is_empty(view.carried()) {
            let button = if keys.swap_offhand.is_key(key) {
                Some(40)
            } else {
                keys.hotbar_key(key).map(|i| i as i32)
            };
            if let Some(button) = button {
                send(out, Some(hovered), hovered as i32, button, ClickType::Swap);
                return true;
            }
        }
        if !is_empty(view.slot(hovered)) && keys.drop.is_key(key) {
            let button = i32::from(mods.ctrl);
            send(out, Some(hovered), hovered as i32, button, ClickType::Throw);
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::container::kind::{ContainerLayout, MenuKind};
    use mcr_data::v26_3;

    struct Table {
        slots: Vec<Option<ItemStack>>,
        carried: Option<ItemStack>,
        creative: bool,
        keys: crate::gui::keys::ScreenKeys,
    }

    impl GameView for Table {
        fn screen_keys(&self) -> crate::gui::keys::ScreenKeys {
            self.keys
        }

        fn slot(&self, index: usize) -> Option<&ItemStack> {
            self.slots.get(index)?.as_ref()
        }

        fn carried(&self) -> Option<&ItemStack> {
            self.carried.as_ref()
        }

        fn creative(&self) -> bool {
            self.creative
        }
    }

    fn item(name: &str, count: i32) -> ItemStack {
        let id = v26_3().item_by_name(&format!("minecraft:{name}")).unwrap();
        ItemStack::new(id as i32, count)
    }

    struct Fixture {
        layout: ContainerLayout,
        view: Table,
        machine: ClickMachine,
    }

    const LEFT: i32 = 100;
    const TOP: i32 = 50;

    fn geom(layout: &ContainerLayout) -> SlotGeometry<'_> {
        SlotGeometry {
            left: LEFT,
            top: TOP,
            width: layout.width,
            height: layout.height,
            slots: &layout.slots,
        }
    }

    impl Fixture {
        fn chest() -> Self {
            let layout = ContainerLayout::new(MenuKind::Generic { rows: 3 });
            let n = layout.slots.len();
            Self {
                layout,
                view: Table {
                    slots: vec![None; n],
                    carried: None,
                    creative: false,
                    keys: Default::default(),
                },
                machine: ClickMachine::default(),
            }
        }

        fn at(&self, slot: usize) -> (f32, f32) {
            let s = &self.layout.slots[slot];
            ((LEFT + s.x + 8) as f32, (TOP + s.y + 8) as f32)
        }

        fn press(
            &mut self,
            pos: (f32, f32),
            button: MouseButton,
            mods: Modifiers,
            double: bool,
        ) -> Vec<ContainerClick> {
            let mut out = Vec::new();
            let g = geom(&self.layout);
            self.machine
                .press(&g, &self.view, pos, button, mods, double, &mut out);
            out
        }

        fn release(
            &mut self,
            pos: (f32, f32),
            button: MouseButton,
            mods: Modifiers,
        ) -> Vec<ContainerClick> {
            let mut out = Vec::new();
            let g = geom(&self.layout);
            self.machine
                .release(&g, &self.view, pos, button, mods, &mut out);
            out
        }

        fn drag(&mut self, pos: (f32, f32)) {
            let g = geom(&self.layout);
            self.machine.mouse_move(&g, &self.view, pos);
        }

        fn key(&mut self, pos: (f32, f32), key: Key, mods: Modifiers) -> Vec<ContainerClick> {
            let mut out = Vec::new();
            let g = geom(&self.layout);
            self.machine.key(&g, &self.view, pos, key, mods, &mut out);
            out
        }

        fn click(
            &mut self,
            slot: usize,
            button: MouseButton,
            mods: Modifiers,
        ) -> Vec<ContainerClick> {
            let pos = self.at(slot);
            let mut out = self.press(pos, button, mods, false);
            out.extend(self.release(pos, button, mods));
            out
        }
    }

    fn c(slot: i32, button: i32, mode: ClickType) -> ContainerClick {
        ContainerClick::new(slot, button, mode)
    }

    #[test]
    fn plain_clicks_with_empty_cursor() {
        let mut f = Fixture::chest();
        f.view.slots[4] = Some(item("stone", 10));
        assert_eq!(
            f.click(4, MouseButton::Left, Modifiers::NONE),
            [c(4, 0, ClickType::Pickup)]
        );
        assert_eq!(
            f.click(4, MouseButton::Right, Modifiers::NONE),
            [c(4, 1, ClickType::Pickup)]
        );
    }

    #[test]
    fn shift_click_quick_moves() {
        let mut f = Fixture::chest();
        f.view.slots[30] = Some(item("stone", 10));
        assert_eq!(
            f.click(30, MouseButton::Left, Modifiers::SHIFT),
            [c(30, 0, ClickType::QuickMove)]
        );
        assert_eq!(
            f.click(30, MouseButton::Right, Modifiers::SHIFT),
            [c(30, 1, ClickType::QuickMove)]
        );
    }

    #[test]
    fn middle_click_clones_only_in_creative() {
        let mut f = Fixture::chest();
        f.view.slots[1] = Some(item("stone", 1));
        assert!(f.click(1, MouseButton::Middle, Modifiers::NONE).is_empty());
        f.view.creative = true;
        assert_eq!(
            f.click(1, MouseButton::Middle, Modifiers::NONE),
            [c(1, 2, ClickType::Clone)]
        );
    }

    #[test]
    fn click_outside_throws() {
        let mut f = Fixture::chest();
        assert_eq!(
            f.click_at((LEFT - 5) as f32, (TOP + 20) as f32, MouseButton::Left),
            [c(-999, 0, ClickType::Throw)]
        );
        assert_eq!(
            f.click_at((LEFT - 5) as f32, (TOP + 20) as f32, MouseButton::Right),
            [c(-999, 1, ClickType::Throw)]
        );
    }

    impl Fixture {
        fn click_at(&mut self, x: f32, y: f32, button: MouseButton) -> Vec<ContainerClick> {
            let mut out = self.press((x, y), button, Modifiers::NONE, false);
            out.extend(self.release((x, y), button, Modifiers::NONE));
            out
        }
    }

    #[test]
    fn click_between_slots_does_nothing_until_release_with_item() {
        let mut f = Fixture::chest();
        let gap = ((LEFT + 3) as f32, (TOP + 3) as f32);
        assert!(
            f.press(gap, MouseButton::Left, Modifiers::NONE, false)
                .is_empty()
        );
        assert!(
            f.release(gap, MouseButton::Left, Modifiers::NONE)
                .is_empty()
        );
    }

    #[test]
    fn carried_item_places_on_release_not_press() {
        let mut f = Fixture::chest();
        f.view.carried = Some(item("stone", 5));
        let pos = f.at(2);
        assert!(
            f.press(pos, MouseButton::Left, Modifiers::NONE, false)
                .is_empty()
        );
        assert_eq!(
            f.release(pos, MouseButton::Left, Modifiers::NONE),
            [c(2, 0, ClickType::Pickup)]
        );
    }

    #[test]
    fn carried_item_shift_release_quick_moves() {
        let mut f = Fixture::chest();
        f.view.carried = Some(item("stone", 5));
        let pos = f.at(2);
        f.press(pos, MouseButton::Left, Modifiers::SHIFT, false);
        assert_eq!(
            f.release(pos, MouseButton::Left, Modifiers::SHIFT),
            [c(2, 0, ClickType::QuickMove)]
        );
    }

    #[test]
    fn carried_item_released_outside_drops() {
        let mut f = Fixture::chest();
        f.view.carried = Some(item("stone", 5));
        let out = (LEFT - 20) as f32;
        f.press((out, 60.0), MouseButton::Right, Modifiers::NONE, false);
        assert_eq!(
            f.release((out, 60.0), MouseButton::Right, Modifiers::NONE),
            [c(-999, 1, ClickType::Pickup)]
        );
    }

    #[test]
    fn hotbar_and_offhand_swap_keys() {
        let mut f = Fixture::chest();
        let pos = f.at(10);
        for d in 1..=9u8 {
            assert_eq!(
                f.key(pos, Key::Digit(d), Modifiers::NONE),
                [c(10, i32::from(d) - 1, ClickType::Swap)]
            );
        }
        assert_eq!(
            f.key(pos, Key::Letter('f'), Modifiers::NONE),
            [c(10, 40, ClickType::Swap)]
        );
        assert!(f.key(pos, Key::Digit(0), Modifiers::NONE).is_empty());
        f.view.carried = Some(item("stone", 1));
        assert!(f.key(pos, Key::Digit(1), Modifiers::NONE).is_empty());
    }

    #[test]
    fn rebound_keys_replace_the_defaults() {
        use crate::gui::keys::ScreenKey;
        let mut f = Fixture::chest();
        f.view.keys.hotbar[0] = ScreenKey::key(Key::Letter('z'), "key.keyboard.z");
        f.view.keys.swap_offhand = ScreenKey::key(Key::Letter('g'), "key.keyboard.g");
        f.view.keys.drop = ScreenKey::key(Key::Letter('x'), "key.keyboard.x");
        f.view.slots[10] = Some(item("stone", 3));
        let pos = f.at(10);
        assert!(f.key(pos, Key::Digit(1), Modifiers::NONE).is_empty());
        assert!(f.key(pos, Key::Letter('f'), Modifiers::NONE).is_empty());
        assert!(f.key(pos, Key::Letter('q'), Modifiers::NONE).is_empty());
        assert_eq!(
            f.key(pos, Key::Letter('z'), Modifiers::NONE),
            [c(10, 0, ClickType::Swap)]
        );
        assert_eq!(
            f.key(pos, Key::Letter('g'), Modifiers::NONE),
            [c(10, 40, ClickType::Swap)]
        );
        assert_eq!(
            f.key(pos, Key::Letter('x'), Modifiers::NONE),
            [c(10, 0, ClickType::Throw)]
        );
    }

    #[test]
    fn swap_key_needs_a_hovered_slot() {
        let mut f = Fixture::chest();
        assert!(f.key((0.0, 0.0), Key::Digit(1), Modifiers::NONE).is_empty());
    }

    #[test]
    fn drop_keys() {
        let mut f = Fixture::chest();
        f.view.slots[7] = Some(item("stone", 3));
        let pos = f.at(7);
        assert_eq!(
            f.key(pos, Key::Letter('q'), Modifiers::NONE),
            [c(7, 0, ClickType::Throw)]
        );
        assert_eq!(
            f.key(pos, Key::Letter('q'), Modifiers::CTRL),
            [c(7, 1, ClickType::Throw)]
        );
        let empty = f.at(8);
        assert!(f.key(empty, Key::Letter('q'), Modifiers::NONE).is_empty());
    }

    #[test]
    fn double_click_picks_up_all() {
        let mut f = Fixture::chest();
        f.view.slots[3] = Some(item("stone", 3));
        let pos = f.at(3);
        f.press(pos, MouseButton::Left, Modifiers::NONE, false);
        f.release(pos, MouseButton::Left, Modifiers::NONE);
        f.view.carried = Some(item("stone", 3));
        f.view.slots[3] = None;
        let down = f.press(pos, MouseButton::Left, Modifiers::NONE, true);
        assert!(down.is_empty());
        assert_eq!(
            f.release(pos, MouseButton::Left, Modifiers::NONE),
            [c(3, 0, ClickType::PickupAll)]
        );
    }

    #[test]
    fn double_click_on_another_slot_is_not_pick_all() {
        let mut f = Fixture::chest();
        f.view.carried = Some(item("stone", 3));
        let (a, b) = (f.at(3), f.at(4));
        f.press(a, MouseButton::Left, Modifiers::NONE, false);
        f.release(a, MouseButton::Left, Modifiers::NONE);
        f.press(b, MouseButton::Left, Modifiers::NONE, true);
        assert_eq!(
            f.release(b, MouseButton::Left, Modifiers::NONE),
            [c(4, 0, ClickType::Pickup)]
        );
    }

    #[test]
    fn shift_double_click_quick_moves_all_matching_in_same_container() {
        let mut f = Fixture::chest();
        f.view.slots[1] = Some(item("stone", 10));
        f.view.slots[5] = Some(item("stone", 20));
        f.view.slots[6] = Some(item("dirt", 4));
        f.view.slots[30] = Some(item("stone", 7));
        let pos = f.at(1);
        let first = f.press(pos, MouseButton::Left, Modifiers::SHIFT, false);
        assert_eq!(first, [c(1, 0, ClickType::QuickMove)]);
        f.release(pos, MouseButton::Left, Modifiers::SHIFT);
        f.press(pos, MouseButton::Left, Modifiers::SHIFT, true);
        let out = f.release(pos, MouseButton::Left, Modifiers::SHIFT);
        assert_eq!(
            out,
            [c(1, 0, ClickType::QuickMove), c(5, 0, ClickType::QuickMove)]
        );
    }

    #[test]
    fn result_slots_do_not_pick_all() {
        let layout = ContainerLayout::new(MenuKind::Crafting);
        let n = layout.slots.len();
        let mut f = Fixture {
            layout,
            view: Table {
                slots: vec![None; n],
                carried: Some(item("stone", 1)),
                creative: false,
                keys: Default::default(),
            },
            machine: ClickMachine::default(),
        };
        let pos = f.at(0);
        f.press(pos, MouseButton::Left, Modifiers::NONE, false);
        f.release(pos, MouseButton::Left, Modifiers::NONE);
        f.press(pos, MouseButton::Left, Modifiers::NONE, true);
        assert_eq!(
            f.release(pos, MouseButton::Left, Modifiers::NONE),
            [c(0, 0, ClickType::Pickup)]
        );
    }

    fn drag_over(f: &mut Fixture, button: MouseButton, slots: &[usize]) -> Vec<ContainerClick> {
        let start = f.at(slots[0]);
        let mut out = f.press(start, button, Modifiers::NONE, false);
        for &s in slots {
            let p = f.at(s);
            f.drag(p);
        }
        let end = f.at(*slots.last().unwrap());
        out.extend(f.release(end, button, Modifiers::NONE));
        out
    }

    #[test]
    fn left_drag_sequence() {
        let mut f = Fixture::chest();
        f.view.carried = Some(item("stone", 10));
        assert_eq!(
            drag_over(&mut f, MouseButton::Left, &[2, 3, 4]),
            [
                c(-999, 0, ClickType::QuickCraft),
                c(2, 1, ClickType::QuickCraft),
                c(3, 1, ClickType::QuickCraft),
                c(4, 1, ClickType::QuickCraft),
                c(-999, 2, ClickType::QuickCraft),
            ]
        );
    }

    #[test]
    fn right_drag_sequence() {
        let mut f = Fixture::chest();
        f.view.carried = Some(item("stone", 10));
        assert_eq!(
            drag_over(&mut f, MouseButton::Right, &[2, 3]),
            [
                c(-999, 4, ClickType::QuickCraft),
                c(2, 5, ClickType::QuickCraft),
                c(3, 5, ClickType::QuickCraft),
                c(-999, 6, ClickType::QuickCraft),
            ]
        );
    }

    #[test]
    fn middle_drag_needs_creative() {
        let mut f = Fixture::chest();
        f.view.carried = Some(item("stone", 10));
        f.view.creative = true;
        assert_eq!(
            drag_over(&mut f, MouseButton::Middle, &[2, 3]),
            [
                c(-999, 8, ClickType::QuickCraft),
                c(2, 9, ClickType::QuickCraft),
                c(3, 9, ClickType::QuickCraft),
                c(-999, 10, ClickType::QuickCraft),
            ]
        );
        let mut g = Fixture::chest();
        g.view.carried = Some(item("stone", 10));
        assert_eq!(
            drag_over(&mut g, MouseButton::Middle, &[2, 3]),
            [c(3, 2, ClickType::Clone)]
        );
    }

    #[test]
    fn drag_over_one_slot_still_sends_the_full_sequence() {
        let mut f = Fixture::chest();
        f.view.carried = Some(item("stone", 10));
        let out = drag_over(&mut f, MouseButton::Left, &[2]);
        assert_eq!(out.len(), 3);
        assert_eq!(out[1], c(2, 1, ClickType::QuickCraft));
    }

    #[test]
    fn drag_skips_slots_holding_other_items_and_repeats() {
        let mut f = Fixture::chest();
        f.view.carried = Some(item("stone", 10));
        f.view.slots[3] = Some(item("dirt", 1));
        f.view.slots[4] = Some(item("stone", 5));
        let out = drag_over(&mut f, MouseButton::Left, &[2, 3, 2, 4]);
        let added: Vec<i32> = out
            .iter()
            .filter(|c| c.button == 1)
            .map(|c| c.slot)
            .collect();
        assert_eq!(added, [2, 4]);
    }

    #[test]
    fn drag_stops_when_out_of_items() {
        let mut f = Fixture::chest();
        f.view.carried = Some(item("stone", 2));
        let out = drag_over(&mut f, MouseButton::Left, &[2, 3, 4]);
        let added: Vec<i32> = out
            .iter()
            .filter(|c| c.button == 1)
            .map(|c| c.slot)
            .collect();
        assert_eq!(added, [2, 3]);
    }

    #[test]
    fn drag_never_targets_result_slots() {
        let layout = ContainerLayout::new(MenuKind::Crafting);
        let n = layout.slots.len();
        let mut f = Fixture {
            layout,
            view: Table {
                slots: vec![None; n],
                carried: Some(item("stone", 8)),
                creative: false,
                keys: Default::default(),
            },
            machine: ClickMachine::default(),
        };
        let out = drag_over(&mut f, MouseButton::Left, &[0, 1, 2]);
        let added: Vec<i32> = out
            .iter()
            .filter(|c| c.button == 1)
            .map(|c| c.slot)
            .collect();
        assert_eq!(added, [1, 2]);
    }

    #[test]
    fn releasing_a_different_button_cancels_the_drag() {
        let mut f = Fixture::chest();
        f.view.carried = Some(item("stone", 10));
        let a = f.at(2);
        f.press(a, MouseButton::Left, Modifiers::NONE, false);
        f.drag(a);
        assert!(f.release(a, MouseButton::Right, Modifiers::NONE).is_empty());
        assert!(!f.machine.quick_crafting());
    }

    #[test]
    fn placement_counts() {
        let stone = item("stone", 10);
        assert_eq!(quick_craft_place_count(3, 0, &stone), 3);
        assert_eq!(quick_craft_place_count(3, 1, &stone), 1);
        assert_eq!(quick_craft_place_count(3, 2, &stone), 64);
        assert_eq!(quick_craft_mask(2, 2), 10);
    }

    #[test]
    fn armor_slots_only_accept_matching_equipment() {
        let layout = ContainerLayout::new(MenuKind::PlayerInventory);
        let head = layout.slots[5];
        assert!(may_place(&head, &item("iron_helmet", 1)));
        assert!(!may_place(&head, &item("iron_boots", 1)));
        assert!(!may_place(&head, &item("stone", 1)));
        assert!(!may_place(&layout.slots[0], &item("stone", 1)));
        assert!(may_place(&layout.slots[9], &item("stone", 1)));
    }
}
