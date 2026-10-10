use mcr_protocol::play::serverbound::ContainerInput as I;
use mcr_protocol::play::serverbound::ContainerInput;
use mcr_protocol::types::Component;
use mcr_protocol::types::ItemStack;

use crate::inventory::{Inventory, OUTSIDE_SLOT};

fn cx() -> crate::hashing::HashCx<'static> {
    static REG: std::sync::OnceLock<crate::registry::Registries> = std::sync::OnceLock::new();
    crate::hashing::HashCx::new(
        mcr_data::v26_3(),
        REG.get_or_init(crate::registry::Registries::default),
    )
}

fn item(name: &str) -> i32 {
    mcr_data::v26_3().item_by_name(name).unwrap() as i32
}

fn open(menu: &str, slots: usize) -> Inventory {
    let menu = mcr_data::v26_3()
        .registry("minecraft:menu")
        .unwrap()
        .id_of(menu)
        .unwrap() as i32;
    let mut inv = Inventory::default();
    inv.open_container(1, menu, Component::text(""));
    inv.set_content(
        1,
        0,
        vec![ItemStack::default(); slots],
        ItemStack::default(),
    );
    inv
}

fn click(inv: &mut Inventory, slot: i16, button: i8, input: ContainerInput) {
    inv.predict_click(&cx(), false, 1, slot, button, input);
}

#[test]
fn shift_click_moves_between_chest_and_inventory() {
    let mut inv = open("minecraft:generic_9x3", 63);
    let stone = item("minecraft:stone");
    inv.open.as_mut().unwrap().slots[54] = ItemStack::new(stone, 40);
    click(&mut inv, 54, 0, ContainerInput::QuickMove);
    let c = inv.open.as_ref().unwrap();
    assert_eq!(c.slots[0].count, 40);
    assert!(c.slots[54].is_empty());
    click(&mut inv, 0, 0, ContainerInput::QuickMove);
    let c = inv.open.as_ref().unwrap();
    assert_eq!(c.slots[62].item, stone);
    assert!(c.slots[0].is_empty());
}

#[test]
fn drag_splits_evenly_and_keeps_remainder() {
    let mut inv = open("minecraft:generic_9x3", 63);
    inv.cursor = ItemStack::new(item("minecraft:stone"), 10);
    click(&mut inv, OUTSIDE_SLOT, 0, ContainerInput::QuickCraft);
    for s in [0, 1, 2] {
        click(&mut inv, s, 1, ContainerInput::QuickCraft);
    }
    click(&mut inv, OUTSIDE_SLOT, 2, ContainerInput::QuickCraft);
    let c = inv.open.as_ref().unwrap();
    assert_eq!(
        [c.slots[0].count, c.slots[1].count, c.slots[2].count],
        [3, 3, 3]
    );
    assert_eq!(inv.cursor.count, 1);
}

#[test]
fn double_click_gathers_matching_stacks() {
    let mut inv = open("minecraft:generic_9x3", 63);
    let stone = item("minecraft:stone");
    inv.open.as_mut().unwrap().slots[1] = ItemStack::new(stone, 5);
    inv.open.as_mut().unwrap().slots[2] = ItemStack::new(stone, 7);
    inv.cursor = ItemStack::new(stone, 1);
    click(&mut inv, 0, 0, ContainerInput::PickupAll);
    assert_eq!(inv.cursor.count, 13);
}

#[test]
fn shulker_box_rejects_shulker_boxes() {
    let mut inv = open("minecraft:shulker_box", 63);
    inv.cursor = ItemStack::new(item("minecraft:shulker_box"), 1);
    click(&mut inv, 3, 0, ContainerInput::Pickup);
    assert!(inv.open.as_ref().unwrap().slots[3].is_empty());
}

#[test]
fn shift_click_equips_armor_from_player_menu() {
    let mut inv = Inventory::default();
    inv.player.slots[10] = ItemStack::new(item("minecraft:iron_helmet"), 1);
    inv.predict_click(&cx(), false, 0, 10, 0, ContainerInput::QuickMove);
    assert_eq!(inv.player.slots[5].item, item("minecraft:iron_helmet"));
    assert!(inv.player.slots[10].is_empty());
}

#[test]
fn hostile_counts_and_indices_do_not_panic() {
    let mut inv = open("minecraft:generic_9x3", 63);
    let stone = item("minecraft:stone");
    inv.open.as_mut().unwrap().slots[0] = ItemStack::new(stone, i32::MAX);
    inv.open.as_mut().unwrap().slots[1] = ItemStack::new(stone, i32::MAX);
    inv.cursor = ItemStack::new(stone, i32::MAX);
    for input in [
        I::Pickup,
        I::QuickMove,
        I::Swap,
        I::Clone,
        I::Throw,
        I::QuickCraft,
        I::PickupAll,
    ] {
        for slot in [-999, -1, 0, 1, 62, 63, 64, i16::MAX, i16::MIN] {
            for button in [-128, -1, 0, 1, 2, 8, 40, 127] {
                click(&mut inv, slot, button, input);
            }
        }
    }
}

#[test]
fn shrinking_content_mid_drag_is_safe() {
    let mut inv = open("minecraft:generic_9x3", 63);
    inv.cursor = ItemStack::new(item("minecraft:stone"), 10);
    click(&mut inv, OUTSIDE_SLOT, 0, I::QuickCraft);
    click(&mut inv, 40, 1, I::QuickCraft);
    click(&mut inv, 41, 1, I::QuickCraft);
    inv.set_content(1, 1, vec![ItemStack::default(); 3], ItemStack::default());
    click(&mut inv, OUTSIDE_SLOT, 2, I::QuickCraft);
    click(&mut inv, 50, 0, I::Swap);
    assert!(!inv.set_slot(9, 0, 0, ItemStack::default()));
}
