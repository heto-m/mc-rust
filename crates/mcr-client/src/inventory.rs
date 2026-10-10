use mcr_protocol::play::serverbound::{ChangedSlot, ContainerInput};
use mcr_protocol::types::Component;
use mcr_protocol::types::ItemStack;
use mcr_protocol::types::item::HashedStack;
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
    pub mount: Option<crate::menu::MountInfo>,
    pub synced: bool,
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
            mount: None,
            synced: false,
        }
    }
}

/// The player menu is always present; `open` holds the server-opened container, if any.
#[derive(Debug, Clone, PartialEq)]
pub struct Inventory {
    pub player: Container,
    pub open: Option<Container>,
    pub cursor: ItemStack,
    pub(crate) qc: crate::menu::QuickCraft,
    /// Slots the server overwrote with something other than the local prediction.
    pub corrections: u64,
    /// Every `container_set_slot`/`container_set_content` applied.
    pub server_updates: u64,
    /// Full `container_set_content` packets for a container that was already synced.
    pub resyncs: u64,
    /// Recent server slot updates, oldest first; tests clear it between clicks.
    pub update_log: Vec<ServerUpdate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerUpdate {
    pub container: i32,
    /// `-1` for a full content packet.
    pub slot: i32,
    pub differs: bool,
}

const MAX_UPDATE_LOG: usize = 1024;

impl Default for Inventory {
    fn default() -> Self {
        Self {
            player: Container::new(PLAYER_MENU_ID, PLAYER_MENU_SLOTS),
            open: None,
            cursor: ItemStack::default(),
            qc: crate::menu::QuickCraft::default(),
            corrections: 0,
            server_updates: 0,
            resyncs: 0,
            update_log: Vec::new(),
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
        self.qc = crate::menu::QuickCraft::default();
    }

    pub fn close(&mut self, id: i32) -> bool {
        if self.open.as_ref().is_some_and(|c| c.id == id) {
            self.open = None;
            self.qc = crate::menu::QuickCraft::default();
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
        self.server_updates += 1;
        let Some(container) = self.container_mut(id) else {
            return false;
        };
        let resync = std::mem::replace(&mut container.synced, true);
        self.resyncs += u64::from(resync);
        let Some(container) = self.container_mut(id) else {
            return false;
        };
        let mut items = items;
        items.truncate(MAX_CONTAINER_SLOTS);
        if id == PLAYER_MENU_ID {
            items.resize(PLAYER_MENU_SLOTS, ItemStack::default());
        }
        container.state_id = state_id;
        let differing = container
            .slots
            .iter()
            .zip(items.iter())
            .filter(|(a, b)| a != b)
            .count() as u64;
        container.slots = items;
        self.corrections += differing;
        self.log_update(id, -1, differing > 0);
        self.cursor = cursor;
        self.qc = crate::menu::QuickCraft::default();
        if id != PLAYER_MENU_ID {
            self.mirror_player_slots();
        }
        true
    }

    pub fn set_slot(&mut self, id: i32, state_id: i32, slot: i16, item: ItemStack) -> bool {
        self.server_updates += 1;
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
        let differs = *target != item;
        *target = item;
        self.corrections += u64::from(differs);
        self.log_update(id, i32::from(slot), differs);
        if id != PLAYER_MENU_ID {
            self.mirror_player_slots();
        }
        true
    }

    fn log_update(&mut self, container: i32, slot: i32, differs: bool) {
        if self.update_log.len() >= MAX_UPDATE_LOG {
            self.update_log.remove(0);
        }
        self.update_log.push(ServerUpdate {
            container,
            slot,
            differs,
        });
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

fn clamp_count(stack: &mut ItemStack) {
    if stack.count <= 0 {
        *stack = ItemStack::default();
    } else {
        stack.count = stack.count.min(MAX_STACK_COUNT);
    }
}

pub const MAX_STACK_COUNT: i32 = 99;

/// Outcome of predicting a click locally.
#[derive(Debug, Clone, PartialEq)]
pub struct ClickPrediction {
    pub changed_slots: Vec<ChangedSlot>,
    pub carried: HashedStack,
}

impl Inventory {
    /// Applies the click to the local copy the way vanilla's `AbstractContainerMenu#clicked` does and
    /// describes the result. Menus with recipe-dependent slots leave the local state untouched.
    pub fn predict_click(
        &mut self,
        ctx: &crate::hashing::HashCx<'_>,
        creative: bool,
        container_id: i32,
        slot: i16,
        button: i8,
        input: ContainerInput,
    ) -> ClickPrediction {
        let mut changed: Vec<usize> = Vec::new();
        if let Some(container) = self.container(container_id).cloned() {
            let before = container.slots.clone();
            let mut slots = container.slots.clone();
            let mut cursor = self.cursor.clone();
            let mut offhand = self
                .player
                .slots
                .get(OFFHAND_MENU_SLOT)
                .cloned()
                .unwrap_or_default();
            slots
                .iter_mut()
                .chain([&mut cursor, &mut offhand])
                .for_each(clamp_count);
            let layout = crate::menu::Layout::from_menu(
                ctx.data,
                container_id,
                container.menu_type,
                slots.len(),
                container.mount,
            );
            let n = slots.len();
            let is_player = container_id == PLAYER_MENU_ID;
            let hotbar = move |b: i8| {
                let b = usize::try_from(b).ok()?;
                if is_player {
                    Some(HOTBAR_START + b)
                } else {
                    n.checked_sub(9).map(|s| s + b)
                }
            };
            let mut qc = std::mem::take(&mut self.qc);
            let unknown = {
                let mut menu = crate::menu::Menu {
                    rules: crate::menu::Rules {
                        data: ctx.data,
                        registries: ctx.registries,
                        creative,
                    },
                    layout,
                    slots: &mut slots,
                    cursor: &mut cursor,
                    offhand: &mut offhand,
                    qc: &mut qc,
                    data: &container.data,
                    unknown: false,
                };
                menu.click(slot, button, input, &hotbar);
                menu.unknown
            };
            self.qc = qc;
            if !unknown {
                changed = (0..slots.len())
                    .filter(|&i| before.get(i) != slots.get(i))
                    .collect();
                self.cursor = cursor;
                if !is_player && let Some(o) = self.player.slots.get_mut(OFFHAND_MENU_SLOT) {
                    *o = offhand;
                }
                if let Some(c) = self.container_mut(container_id) {
                    c.slots = slots;
                }
                if !is_player {
                    self.mirror_player_slots();
                }
            }
        }
        let container = self.container(container_id);
        let changed_slots = changed
            .into_iter()
            .filter_map(|i| {
                let item = container?.slots.get(i)?;
                Some(ChangedSlot {
                    slot: i as i16,
                    item: ctx.hash_stack(item),
                })
            })
            .collect();
        ClickPrediction {
            changed_slots,
            carried: ctx.hash_stack(&self.cursor),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stack(item: i32, count: i32) -> ItemStack {
        ItemStack::new(item, count)
    }

    fn cx() -> crate::hashing::HashCx<'static> {
        static REG: std::sync::OnceLock<crate::registry::Registries> = std::sync::OnceLock::new();
        crate::hashing::HashCx::new(
            mcr_data::v26_3(),
            REG.get_or_init(crate::registry::Registries::default),
        )
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
        let p = inv.predict_click(&cx(), false, 0, 10, 0, ContainerInput::Pickup);
        assert_eq!(inv.cursor.count, 10);
        assert!(inv.player.slots[10].is_empty());
        assert_eq!(p.changed_slots.len(), 1);
        inv.predict_click(&cx(), false, 0, 11, 1, ContainerInput::Pickup);
        assert_eq!(inv.player.slots[11].count, 1);
        assert_eq!(inv.cursor.count, 9);
    }

    #[test]
    fn right_click_takes_the_larger_half() {
        let mut inv = Inventory::default();
        inv.player.slots[10] = stack(1, 5);
        inv.predict_click(&cx(), false, 0, 10, 1, ContainerInput::Pickup);
        assert_eq!((inv.cursor.count, inv.player.slots[10].count), (3, 2));
    }

    #[test]
    fn hotbar_swap_exchanges_slots() {
        let mut inv = Inventory::default();
        inv.player.slots[10] = stack(1, 2);
        inv.player.slots[36] = stack(2, 3);
        let p = inv.predict_click(&cx(), false, 0, 10, 0, ContainerInput::Swap);
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
        let p = inv.predict_click(&cx(), false, 0, OUTSIDE_SLOT, 0, ContainerInput::Pickup);
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
