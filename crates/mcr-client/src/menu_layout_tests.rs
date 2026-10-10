use mcr_buf::{Identifier, VarInt};
use mcr_protocol::play::clientbound::ui::Pairs;
use mcr_protocol::play::clientbound::ui::UpdateRecipes;
use mcr_protocol::play::serverbound::ContainerInput as I;
use mcr_protocol::types::Component;
use mcr_protocol::types::ItemStack;

use crate::inventory::Inventory;
use crate::registry::Registries;

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

fn registries_with(sets: &[(&str, &[&str])]) -> Registries {
    let mut r = Registries::default();
    r.apply_recipes(&UpdateRecipes {
        item_sets: Pairs(
            sets.iter()
                .map(|(key, items)| {
                    (
                        Identifier::parse(key).unwrap(),
                        items.iter().map(|i| VarInt(item(i))).collect(),
                    )
                })
                .collect(),
        ),
        stonecutter_recipes: vec![],
    });
    r
}

fn click(inv: &mut Inventory, regs: &Registries, id: i32, slot: i16, input: I) {
    let cx = crate::hashing::HashCx::new(mcr_data::v26_3(), regs);
    inv.predict_click(&cx, false, id, slot, 0, input);
}

#[test]
fn crafting_result_pickup_consumes_ingredients_and_leaves_remainders() {
    let mut inv = open("minecraft:crafting", 46);
    let regs = registries_with(&[]);
    let c = inv.open.as_mut().unwrap();
    c.slots[0] = ItemStack::new(item("minecraft:cake"), 1);
    c.slots[1] = ItemStack::new(item("minecraft:milk_bucket"), 1);
    c.slots[2] = ItemStack::new(item("minecraft:sugar"), 2);
    c.slots[5] = ItemStack::new(item("minecraft:wheat"), 1);
    click(&mut inv, &regs, 1, 0, I::Pickup);
    let c = inv.open.as_ref().unwrap();
    assert_eq!(inv.cursor.item, item("minecraft:cake"));
    assert!(c.slots[0].is_empty());
    assert_eq!(c.slots[1].item, item("minecraft:bucket"));
    assert_eq!(c.slots[2].count, 1);
    assert!(c.slots[5].is_empty());
}

#[test]
fn furnace_shift_click_routes_by_recipe_set_and_fuel() {
    let mut inv = open("minecraft:furnace", 39);
    let regs = registries_with(&[("minecraft:furnace_input", &["minecraft:raw_iron"])]);
    let c = inv.open.as_mut().unwrap();
    c.slots[3] = ItemStack::new(item("minecraft:raw_iron"), 5);
    c.slots[4] = ItemStack::new(item("minecraft:coal"), 5);
    c.slots[5] = ItemStack::new(item("minecraft:stone"), 5);
    click(&mut inv, &regs, 1, 3, I::QuickMove);
    click(&mut inv, &regs, 1, 4, I::QuickMove);
    click(&mut inv, &regs, 1, 5, I::QuickMove);
    let c = inv.open.as_ref().unwrap();
    assert_eq!(c.slots[0].item, item("minecraft:raw_iron"));
    assert_eq!(c.slots[1].item, item("minecraft:coal"));
    assert!(c.slots[5].is_empty());
    assert_eq!(c.slots[30].item, item("minecraft:stone"));
}

#[test]
fn enchanting_table_takes_one_item_and_lapis() {
    let mut inv = open("minecraft:enchantment", 38);
    let regs = registries_with(&[]);
    let c = inv.open.as_mut().unwrap();
    c.slots[2] = ItemStack::new(item("minecraft:iron_sword"), 1);
    c.slots[3] = ItemStack::new(item("minecraft:lapis_lazuli"), 7);
    click(&mut inv, &regs, 1, 2, I::QuickMove);
    click(&mut inv, &regs, 1, 3, I::QuickMove);
    let c = inv.open.as_ref().unwrap();
    assert_eq!(c.slots[0].item, item("minecraft:iron_sword"));
    assert_eq!(c.slots[1].count, 7);
}

#[test]
fn grindstone_result_pickup_clears_both_inputs() {
    let mut inv = open("minecraft:grindstone", 39);
    let regs = registries_with(&[]);
    let c = inv.open.as_mut().unwrap();
    c.slots[0] = ItemStack::new(item("minecraft:iron_sword"), 1);
    c.slots[1] = ItemStack::new(item("minecraft:iron_sword"), 1);
    c.slots[2] = ItemStack::new(item("minecraft:iron_sword"), 1);
    click(&mut inv, &regs, 1, 2, I::Pickup);
    let c = inv.open.as_ref().unwrap();
    assert_eq!(inv.cursor.item, item("minecraft:iron_sword"));
    assert!(c.slots[0].is_empty() && c.slots[1].is_empty() && c.slots[2].is_empty());
}

#[test]
fn anvil_result_is_left_to_the_server() {
    let mut inv = open("minecraft:anvil", 39);
    let regs = registries_with(&[]);
    inv.open.as_mut().unwrap().slots[2] = ItemStack::new(item("minecraft:iron_sword"), 1);
    click(&mut inv, &regs, 1, 2, I::Pickup);
    assert!(inv.cursor.is_empty());
}

#[test]
fn player_head_shift_clicks_into_the_helmet_slot() {
    let mut inv = Inventory::default();
    let regs = registries_with(&[]);
    inv.player.slots[36] = ItemStack::new(item("minecraft:player_head"), 1);
    click(&mut inv, &regs, 0, 36, I::QuickMove);
    assert_eq!(inv.player.slots[5].item, item("minecraft:player_head"));
}
