use mcr_buf::LengthPrefixed;
use mcr_buf::VarInt;
use mcr_data::Version;
use mcr_protocol::play::serverbound::{ChangedSlot, ContainerInput};
use mcr_protocol::types::Component;
use mcr_protocol::types::ItemStack;
use mcr_protocol::types::item::{HashedItem, HashedPatchMap, HashedStack};
use rustc_hash::FxHashMap;

pub const PLAYER_MENU_ID: i32 = 0;
pub const PLAYER_MENU_SLOTS: usize = 46;
pub const MAX_CONTAINER_SLOTS: usize = 4096;
pub const OUTSIDE_SLOT: i16 = -999;
pub const HOTBAR_START: usize = 36;
pub const OFFHAND_MENU_SLOT: usize = 45;

#[derive(Debug, Clone, PartialEq)]
pub struct Container {
    pub id: i32,
    pub state_id: i32,
    pub slots: Vec<ItemStack>,
    pub menu_type: Option<i32>,
    pub title: Component,
    pub data: FxHashMap<i16, i16>,
}

impl Container {
    fn new(id: i32, slots: usize) -> Self {
        Self {
            id,
            state_id: 0,
            slots: vec![ItemStack::default(); slots],
            menu_type: None,
            title: Component::text(""),
            data: FxHashMap::default(),
        }
    }
}

/// The player menu is always present; `open` holds the server-opened container, if any.
#[derive(Debug, Clone, PartialEq)]
pub struct Inventory {
    pub player: Container,
    pub open: Option<Container>,
    pub cursor: ItemStack,
}

impl Default for Inventory {
    fn default() -> Self {
        Self {
            player: Container::new(PLAYER_MENU_ID, PLAYER_MENU_SLOTS),
            open: None,
            cursor: ItemStack::default(),
        }
    }
}

/// Player inventory index (hotbar 0-8, main 9-35, armor 36-39 feet to head, offhand 40) to menu slot.
pub fn inventory_index_to_menu_slot(index: i32) -> Option<usize> {
    Some(match index {
        0..=8 => HOTBAR_START + index as usize,
        9..=35 => index as usize,
        36..=39 => 8 - (index as usize - 36),
        40 => OFFHAND_MENU_SLOT,
        _ => return None,
    })
}

impl Inventory {
    pub fn container(&self, id: i32) -> Option<&Container> {
        if id == PLAYER_MENU_ID {
            Some(&self.player)
        } else {
            self.open.as_ref().filter(|c| c.id == id)
        }
    }

    fn container_mut(&mut self, id: i32) -> Option<&mut Container> {
        if id == PLAYER_MENU_ID {
            Some(&mut self.player)
        } else {
            self.open.as_mut().filter(|c| c.id == id)
        }
    }

    pub fn hotbar(&self, slot: u8) -> &ItemStack {
        &self.player.slots[HOTBAR_START + usize::from(slot.min(8))]
    }

    pub fn open_container(&mut self, id: i32, menu_type: i32, title: Component) {
        let mut container = Container::new(id, 0);
        container.menu_type = Some(menu_type);
        container.title = title;
        self.open = Some(container);
    }

    pub fn close(&mut self, id: i32) -> bool {
        if self.open.as_ref().is_some_and(|c| c.id == id) {
            self.open = None;
            true
        } else {
            false
        }
    }

    pub fn set_content(
        &mut self,
        id: i32,
        state_id: i32,
        items: Vec<ItemStack>,
        cursor: ItemStack,
    ) -> bool {
        self.cursor = cursor;
        let Some(container) = self.container_mut(id) else {
            return false;
        };
        let mut items = items;
        items.truncate(MAX_CONTAINER_SLOTS);
        if id == PLAYER_MENU_ID {
            items.resize(PLAYER_MENU_SLOTS, ItemStack::default());
        }
        container.state_id = state_id;
        container.slots = items;
        if id != PLAYER_MENU_ID {
            self.mirror_player_slots();
        }
        true
    }

    pub fn set_slot(&mut self, id: i32, state_id: i32, slot: i16, item: ItemStack) -> bool {
        let Some(container) = self.container_mut(id) else {
            return false;
        };
        container.state_id = state_id;
        let Some(target) = usize::try_from(slot)
            .ok()
            .and_then(|i| container.slots.get_mut(i))
        else {
            return false;
        };
        *target = item;
        if id != PLAYER_MENU_ID {
            self.mirror_player_slots();
        }
        true
    }

    pub fn set_player_inventory(&mut self, index: i32, item: ItemStack) -> bool {
        let Some(slot) = inventory_index_to_menu_slot(index) else {
            return false;
        };
        self.player.slots[slot] = item;
        true
    }

    /// An open container's last 36 slots are the player's main inventory and hotbar.
    fn mirror_player_slots(&mut self) {
        let Some(open) = &self.open else {
            return;
        };
        let n = open.slots.len();
        if n < 36 {
            return;
        }
        for i in 0..27 {
            self.player.slots[9 + i] = open.slots[n - 36 + i].clone();
        }
        for i in 0..9 {
            self.player.slots[HOTBAR_START + i] = open.slots[n - 9 + i].clone();
        }
    }
}

pub fn same_item(a: &ItemStack, b: &ItemStack) -> bool {
    a.item == b.item && a.components == b.components
}

fn max_stack(data: &Version, item: &ItemStack) -> i32 {
    u32::try_from(item.item)
        .ok()
        .and_then(|i| data.item_info(i))
        .map_or(64, |info| i32::from(info.max_stack_size).max(1))
}

pub fn hash_stack(stack: &ItemStack) -> HashedStack {
    if stack.is_empty() {
        return HashedStack::Empty;
    }
    HashedStack::Item(HashedItem {
        item: stack.item,
        count: stack.count,
        components: HashedPatchMap {
            added: LengthPrefixed(Vec::new()),
            removed: LengthPrefixed(
                stack
                    .components
                    .removed
                    .iter()
                    .map(|&i| VarInt(i))
                    .collect(),
            ),
        },
    })
}

/// Outcome of predicting a click locally.
#[derive(Debug, Clone, PartialEq)]
pub struct ClickPrediction {
    pub changed_slots: Vec<ChangedSlot>,
    pub carried: HashedStack,
}

impl Inventory {
    /// Applies the click to the local copy (pickup, hotbar swap and drop only) and describes the result.
    /// Other modes send no predicted changes, so the server answers with a resync.
    pub fn predict_click(
        &mut self,
        data: &Version,
        container_id: i32,
        slot: i16,
        button: i8,
        input: ContainerInput,
    ) -> ClickPrediction {
        let mut changed: Vec<usize> = Vec::new();
        let result_slot_of_player_menu = container_id == PLAYER_MENU_ID && slot == 0;
        if let Some(container) = self.container(container_id).cloned()
            && !result_slot_of_player_menu
        {
            let mut slots = container.slots;
            let mut cursor = std::mem::take(&mut self.cursor);
            match input {
                ContainerInput::Pickup => {
                    simulate_pickup(data, &mut slots, &mut cursor, slot, button, &mut changed)
                }
                ContainerInput::Swap => simulate_swap(
                    &mut slots,
                    container_id == PLAYER_MENU_ID,
                    slot,
                    button,
                    &mut changed,
                ),
                ContainerInput::Throw => {
                    simulate_throw(&mut slots, slot, button, &mut changed);
                }
                _ => {}
            }
            self.cursor = cursor;
            if let Some(c) = self.container_mut(container_id) {
                c.slots = slots;
            }
        }
        changed.sort_unstable();
        changed.dedup();
        let container = self.container(container_id);
        let changed_slots = changed
            .into_iter()
            .filter_map(|i| {
                let item = container?.slots.get(i)?;
                Some(ChangedSlot {
                    slot: i as i16,
                    item: hash_stack(item),
                })
            })
            .collect();
        ClickPrediction {
            changed_slots,
            carried: hash_stack(&self.cursor),
        }
    }
}

fn take_half(stack: &mut ItemStack) -> ItemStack {
    let taken = (stack.count + 1) / 2;
    let mut out = stack.clone();
    out.count = taken;
    stack.count -= taken;
    if stack.count <= 0 {
        *stack = ItemStack::default();
    }
    out
}

fn split_one(stack: &mut ItemStack) -> ItemStack {
    let mut out = stack.clone();
    out.count = 1;
    stack.count -= 1;
    if stack.count <= 0 {
        *stack = ItemStack::default();
    }
    out
}

fn simulate_pickup(
    data: &Version,
    slots: &mut [ItemStack],
    cursor: &mut ItemStack,
    slot: i16,
    button: i8,
    changed: &mut Vec<usize>,
) {
    if slot == OUTSIDE_SLOT {
        if !cursor.is_empty() {
            if button == 0 {
                *cursor = ItemStack::default();
            } else {
                split_one(cursor);
            }
        }
        return;
    }
    let Some(index) = usize::try_from(slot).ok().filter(|&i| i < slots.len()) else {
        return;
    };
    let target = &mut slots[index];
    match (target.is_empty(), cursor.is_empty()) {
        (true, true) => {}
        (false, true) => {
            *cursor = if button == 0 {
                std::mem::take(target)
            } else {
                take_half(target)
            };
            changed.push(index);
        }
        (true, false) => {
            let limit = max_stack(data, cursor);
            let amount = if button == 0 { cursor.count } else { 1 }.min(limit);
            let mut placed = cursor.clone();
            placed.count = amount;
            cursor.count -= amount;
            if cursor.count <= 0 {
                *cursor = ItemStack::default();
            }
            *target = placed;
            changed.push(index);
        }
        (false, false) => {
            if same_item(target, cursor) {
                let limit = max_stack(data, target);
                let room = (limit - target.count).max(0);
                let amount = if button == 0 { cursor.count } else { 1 }.min(room);
                target.count += amount;
                cursor.count -= amount;
                if cursor.count <= 0 {
                    *cursor = ItemStack::default();
                }
                if amount > 0 {
                    changed.push(index);
                }
            } else if cursor.count <= max_stack(data, cursor) {
                std::mem::swap(target, cursor);
                changed.push(index);
            }
        }
    }
}

fn simulate_swap(
    slots: &mut [ItemStack],
    player_menu: bool,
    slot: i16,
    button: i8,
    changed: &mut Vec<usize>,
) {
    let Some(a) = usize::try_from(slot).ok().filter(|&i| i < slots.len()) else {
        return;
    };
    let n = slots.len();
    let hotbar_start = if player_menu {
        HOTBAR_START
    } else {
        n.saturating_sub(9)
    };
    let b = match button {
        0..=8 if n >= 36 => hotbar_start + button as usize,
        40 if player_menu => OFFHAND_MENU_SLOT,
        _ => return,
    };
    if a == b || b >= n {
        return;
    }
    slots.swap(a, b);
    changed.push(a);
    changed.push(b);
}

fn simulate_throw(slots: &mut [ItemStack], slot: i16, button: i8, changed: &mut Vec<usize>) {
    let Some(index) = usize::try_from(slot).ok().filter(|&i| i < slots.len()) else {
        return;
    };
    if slots[index].is_empty() {
        return;
    }
    if button == 0 {
        split_one(&mut slots[index]);
    } else {
        slots[index] = ItemStack::default();
    }
    changed.push(index);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stack(item: i32, count: i32) -> ItemStack {
        ItemStack::new(item, count)
    }

    fn data() -> &'static Version {
        mcr_data::v26_3()
    }

    #[test]
    fn index_mapping_matches_player_menu_layout() {
        assert_eq!(inventory_index_to_menu_slot(0), Some(36));
        assert_eq!(inventory_index_to_menu_slot(9), Some(9));
        assert_eq!(inventory_index_to_menu_slot(36), Some(8));
        assert_eq!(inventory_index_to_menu_slot(39), Some(5));
        assert_eq!(inventory_index_to_menu_slot(40), Some(45));
        assert_eq!(inventory_index_to_menu_slot(41), None);
    }

    #[test]
    fn pickup_and_place_round_trip() {
        let mut inv = Inventory::default();
        inv.player.slots[10] = stack(1, 10);
        let p = inv.predict_click(data(), 0, 10, 0, ContainerInput::Pickup);
        assert_eq!(inv.cursor.count, 10);
        assert!(inv.player.slots[10].is_empty());
        assert_eq!(p.changed_slots.len(), 1);
        inv.predict_click(data(), 0, 11, 1, ContainerInput::Pickup);
        assert_eq!(inv.player.slots[11].count, 1);
        assert_eq!(inv.cursor.count, 9);
    }

    #[test]
    fn right_click_takes_the_larger_half() {
        let mut inv = Inventory::default();
        inv.player.slots[10] = stack(1, 5);
        inv.predict_click(data(), 0, 10, 1, ContainerInput::Pickup);
        assert_eq!((inv.cursor.count, inv.player.slots[10].count), (3, 2));
    }

    #[test]
    fn hotbar_swap_exchanges_slots() {
        let mut inv = Inventory::default();
        inv.player.slots[10] = stack(1, 2);
        inv.player.slots[36] = stack(2, 3);
        let p = inv.predict_click(data(), 0, 10, 0, ContainerInput::Swap);
        assert_eq!(inv.player.slots[10].item, 2);
        assert_eq!(inv.player.slots[36].item, 1);
        assert_eq!(p.changed_slots.len(), 2);
    }

    #[test]
    fn drop_outside_clears_cursor() {
        let mut inv = Inventory {
            cursor: stack(1, 4),
            ..Inventory::default()
        };
        let p = inv.predict_click(data(), 0, OUTSIDE_SLOT, 0, ContainerInput::Pickup);
        assert!(inv.cursor.is_empty());
        assert_eq!(p.carried, HashedStack::Empty);
    }

    #[test]
    fn open_container_mirrors_player_slots() {
        let mut inv = Inventory::default();
        inv.open_container(3, 2, Component::text("Chest"));
        let mut items = vec![ItemStack::default(); 27 + 36];
        items[27 + 27] = stack(5, 1);
        assert!(inv.set_content(3, 7, items, ItemStack::default()));
        assert_eq!(inv.player.slots[36].item, 5);
        assert_eq!(inv.container(3).unwrap().state_id, 7);
        assert!(!inv.set_slot(9, 1, 0, stack(1, 1)));
    }

    #[test]
    fn oversized_content_is_truncated() {
        let mut inv = Inventory::default();
        inv.open_container(1, 0, Component::text(""));
        let items = vec![ItemStack::default(); MAX_CONTAINER_SLOTS + 100];
        inv.set_content(1, 0, items, ItemStack::default());
        assert_eq!(inv.container(1).unwrap().slots.len(), MAX_CONTAINER_SLOTS);
    }
}
