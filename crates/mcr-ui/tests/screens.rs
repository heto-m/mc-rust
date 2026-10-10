mod common;

use common::harness::{Harness, SIZE, TestView, send, stack, typed};
use mcr_protocol::types::text::Component;
use mcr_ui::{ContainerScreen, Key, MenuKind, MouseButton, Screen, ScreenAction, UiInput};

fn container(name: &str, title: &str) -> ContainerScreen {
    ContainerScreen::new(
        MenuKind::from_name(name).unwrap(),
        Component::translatable(title, Vec::new()),
    )
}

#[test]
fn player_inventory_shot() {
    let Some(mut h) = Harness::new() else { return };
    let mut screen = ContainerScreen::player_inventory();
    let mut view = TestView::with_slots(46);
    view.set(1, "oak_planks", 4);
    view.set(5, "iron_helmet", 1);
    view.set(7, "iron_leggings", 1);
    view.set(9, "cobblestone", 64);
    view.set(10, "diamond_sword", 1);
    view.set(36, "iron_pickaxe", 1);
    view.set(37, "torch", 23);
    view.set(38, "bread", 5);
    view.set(45, "shield", 1);
    view.carried = Some(stack("apple", 3));
    h.shot(
        "screen_inventory.png",
        SIZE,
        &mut screen,
        &view,
        (200.0, 130.0),
    );
}

#[test]
fn chest_shot() {
    let Some(mut h) = Harness::new() else { return };
    let mut screen = ContainerScreen::new(
        MenuKind::Generic { rows: 3 },
        Component::translatable("container.chest", Vec::new()),
    );
    let mut view = TestView::with_slots(63);
    for (i, (name, n)) in [
        ("diamond", 12),
        ("oak_log", 64),
        ("golden_apple", 3),
        ("iron_ingot", 33),
        ("bow", 1),
        ("compass", 1),
        ("redstone", 40),
        ("stone_bricks", 17),
        ("enchanted_golden_apple", 1),
    ]
    .into_iter()
    .enumerate()
    {
        view.set(i * 2, name, n);
    }
    view.set(27, "cobblestone", 64);
    view.set(54, "wooden_sword", 1);
    h.shot(
        "screen_chest.png",
        SIZE,
        &mut screen,
        &view,
        (213.0 - 88.0 + 8.0 + 18.0 * 2.0, 120.0 - 84.0 + 18.0 + 8.0),
    );
}

#[test]
fn furnace_shot() {
    let Some(mut h) = Harness::new() else { return };
    let mut screen = container("furnace", "container.furnace");
    let mut view = TestView::with_slots(39);
    view.set(0, "raw_iron", 5);
    view.set(1, "coal", 7);
    view.set(2, "iron_ingot", 3);
    view.set(30, "cobblestone", 64);
    view.data = vec![120, 200, 60, 200];
    h.shot("screen_furnace.png", SIZE, &mut screen, &view, (10.0, 10.0));
}

#[test]
fn crafting_table_shot() {
    let Some(mut h) = Harness::new() else { return };
    let mut screen = ContainerScreen::new(
        MenuKind::Crafting,
        Component::translatable("container.crafting", Vec::new()),
    );
    let mut view = TestView::with_slots(46);
    view.set(0, "stick", 4);
    view.set(1, "oak_planks", 1);
    view.set(4, "oak_planks", 1);
    view.set(10, "oak_planks", 64);
    view.set(37, "crafting_table", 1);
    h.shot(
        "screen_crafting.png",
        SIZE,
        &mut screen,
        &view,
        (10.0, 10.0),
    );
}

#[test]
fn brewing_stand_shot() {
    let Some(mut h) = Harness::new() else { return };
    let mut screen = container("brewing_stand", "container.brewing");
    let mut view = TestView::with_slots(41);
    view.set(0, "potion", 1);
    view.set(1, "potion", 1);
    view.set(3, "nether_wart", 12);
    view.set(4, "blaze_powder", 5);
    view.data = vec![220, 12, 400, 20];
    h.shot("screen_brewing.png", SIZE, &mut screen, &view, (10.0, 10.0));
}

#[test]
fn enchanting_shot() {
    let Some(mut h) = Harness::new() else { return };
    let mut screen = container("enchantment", "container.enchant");
    let mut view = TestView::with_slots(38);
    view.set(0, "diamond_sword", 1);
    view.set(1, "lapis_lazuli", 2);
    view.data = vec![8, 17, 30, 1234, 0, 0, 0, 0, 0, 0];
    view.xp = 20;
    h.shot(
        "screen_enchanting.png",
        SIZE,
        &mut screen,
        &view,
        (213.0 - 88.0 + 70.0, 120.0 - 83.0 + 33.0),
    );
}

#[test]
fn anvil_shot() {
    let Some(mut h) = Harness::new() else { return };
    let mut screen = container("anvil", "container.repair");
    let mut view = TestView::with_slots(39);
    view.set(0, "iron_sword", 1);
    view.set(1, "iron_ingot", 2);
    view.set(2, "iron_sword", 1);
    view.data = vec![7];
    h.shot("screen_anvil.png", SIZE, &mut screen, &view, (10.0, 10.0));
}

#[test]
fn beacon_shot() {
    let Some(mut h) = Harness::new() else { return };
    let mut screen = container("beacon", "container.beacon");
    let mut view = TestView::with_slots(37);
    view.set(0, "emerald", 1);
    view.data = vec![4, 1, 10];
    h.shot("screen_beacon.png", SIZE, &mut screen, &view, (10.0, 10.0));
}

#[test]
fn merchant_shot() {
    use mcr_ui::{MerchantInfo, TradeOffer};
    let Some(mut h) = Harness::new() else { return };
    let mut screen = container("merchant", "entity.minecraft.villager");
    let mut view = TestView::with_slots(39);
    let offer = |cost: (&str, i32), result: (&str, i32), out: bool| TradeOffer {
        base_cost_a: stack(cost.0, cost.1),
        cost_a: stack(cost.0, cost.1),
        cost_b: Default::default(),
        result: stack(result.0, result.1),
        out_of_stock: out,
    };
    let mut discounted = offer(("emerald", 5), ("iron_pickaxe", 1), false);
    discounted.base_cost_a = stack("emerald", 8);
    view.merchant = Some(MerchantInfo {
        offers: vec![
            offer(("wheat", 20), ("emerald", 1), false),
            discounted,
            offer(("emerald", 1), ("bread", 6), true),
            offer(("emerald", 3), ("cooked_beef", 4), false),
        ],
        level: 2,
        xp: 30,
        future_xp: 10,
        show_progress: true,
        can_restock: true,
    });
    view.set(0, "wheat", 20);
    h.shot(
        "screen_merchant.png",
        SIZE,
        &mut screen,
        &view,
        (213.0 - 138.0 + 20.0, 120.0 - 83.0 + 38.0),
    );
}

#[test]
fn stonecutter_shot() {
    let Some(mut h) = Harness::new() else { return };
    let mut screen = container("stonecutter", "container.stonecutter");
    let mut view = TestView::with_slots(38);
    view.set(0, "cobblestone", 8);
    for name in [
        "cobblestone_slab",
        "cobblestone_stairs",
        "cobblestone_wall",
        "stone_bricks",
        "chiseled_stone_bricks",
    ] {
        view.options.push(stack(name, 1));
    }
    view.data = vec![1];
    h.shot(
        "screen_stonecutter.png",
        SIZE,
        &mut screen,
        &view,
        (213.0 - 88.0 + 52.0 + 40.0, 120.0 - 83.0 + 20.0),
    );
}

#[test]
fn loom_shot() {
    let Some(mut h) = Harness::new() else { return };
    let mut screen = container("loom", "container.loom");
    let mut view = TestView::with_slots(40);
    view.set(0, "white_banner", 1);
    view.set(1, "red_dye", 1);
    view.patterns = 9;
    view.data = vec![2];
    h.shot("screen_loom.png", SIZE, &mut screen, &view, (10.0, 10.0));
}

#[test]
fn smithing_and_grindstone_shots() {
    let Some(mut h) = Harness::new() else { return };
    let mut smithing = container("smithing", "container.upgrade");
    let mut view = TestView::with_slots(40);
    view.set(1, "diamond_chestplate", 1);
    view.set(2, "netherite_ingot", 1);
    view.data = vec![1];
    h.shot(
        "screen_smithing.png",
        SIZE,
        &mut smithing,
        &view,
        (10.0, 10.0),
    );
    let mut grind = container("grindstone", "container.grindstone_title");
    let mut view = TestView::with_slots(39);
    view.set(0, "diamond_pickaxe", 1);
    h.shot(
        "screen_grindstone.png",
        SIZE,
        &mut grind,
        &view,
        (10.0, 10.0),
    );
}

#[test]
fn cartography_hopper_dispenser_shots() {
    let Some(mut h) = Harness::new() else { return };
    let mut screen = container("cartography_table", "container.cartography_table");
    let mut view = TestView::with_slots(39);
    view.set(0, "filled_map", 1);
    view.set(1, "paper", 4);
    h.shot(
        "screen_cartography.png",
        SIZE,
        &mut screen,
        &view,
        (10.0, 10.0),
    );
    let mut hopper = container("hopper", "container.hopper");
    let mut view = TestView::with_slots(41);
    view.set(1, "iron_ingot", 5);
    h.shot("screen_hopper.png", SIZE, &mut hopper, &view, (10.0, 10.0));
    let mut dispenser = container("generic_3x3", "container.dispenser");
    let mut view = TestView::with_slots(45);
    view.set(4, "arrow", 64);
    h.shot(
        "screen_dispenser.png",
        SIZE,
        &mut dispenser,
        &view,
        (10.0, 10.0),
    );
}

#[test]
fn crafter_horse_lectern_shots() {
    let Some(mut h) = Harness::new() else { return };
    let mut crafter = container("crafter_3x3", "container.crafter");
    let mut view = TestView::with_slots(46);
    view.set(0, "iron_ingot", 1);
    view.data = vec![0, 1, 0, 0, 1, 0, 0, 0, 0, 1];
    h.shot(
        "screen_crafter.png",
        SIZE,
        &mut crafter,
        &view,
        (10.0, 10.0),
    );
    let mut horse = ContainerScreen::new(
        MenuKind::Mount {
            chest_columns: 5,
            saddle: true,
            armor: true,
            llama: false,
        },
        Component::text("Horse"),
    );
    let mut view = TestView::with_slots(2 + 15 + 36);
    view.set(0, "saddle", 1);
    view.set(5, "carrot", 12);
    h.shot("screen_horse.png", SIZE, &mut horse, &view, (10.0, 10.0));
    let mut lectern = container("lectern", "container.lectern");
    let mut view = TestView::with_slots(1);
    view.pages = vec![
        Component::text(
            "Chapter 1\n\nIt was a dark and stormy night, and the creeper had been waiting.",
        ),
        Component::text("Page two"),
    ];
    view.data = vec![0];
    h.shot(
        "screen_lectern.png",
        SIZE,
        &mut lectern,
        &view,
        (10.0, 10.0),
    );
}

fn click(x: f32, y: f32) -> [UiInput; 2] {
    [
        UiInput::press(x, y, MouseButton::Left),
        UiInput::release(x, y, MouseButton::Left),
    ]
}

#[test]
fn anvil_rename_is_sent_as_typed() {
    let Some(mut h) = Harness::new() else { return };
    let mut screen = container("anvil", "container.repair");
    let mut view = TestView::with_slots(39);
    view.set(0, "iron_sword", 1);
    h.shot("anvil_rename.png", SIZE, &mut screen, &view, (10.0, 10.0));
    send(&mut screen, &view, &[UiInput::key(Key::End)]);
    let out = send(&mut screen, &view, &typed("X"));
    assert!(
        matches!(out.as_slice(), [ScreenAction::RenameItem(n)] if n == "Iron SwordX"),
        "{out:?}"
    );
    let out = send(&mut screen, &view, &[UiInput::key(Key::Letter('e'))]);
    assert!(out.is_empty(), "typing keys must not close the anvil");
    let out = send(&mut screen, &view, &[UiInput::key(Key::Escape)]);
    assert!(matches!(out.as_slice(), [ScreenAction::CloseContainer]));
}

#[test]
fn anvil_name_box_is_read_only_without_an_item() {
    let Some(mut h) = Harness::new() else { return };
    let mut screen = container("anvil", "container.repair");
    let view = TestView::with_slots(39);
    h.shot("anvil_empty.png", SIZE, &mut screen, &view, (10.0, 10.0));
    assert!(send(&mut screen, &view, &typed("X")).is_empty());
}

#[test]
fn beacon_buttons_follow_levels_and_confirm_sends_effects() {
    let Some(mut h) = Harness::new() else { return };
    let mut screen = container("beacon", "container.beacon");
    let mut view = TestView::with_slots(37);
    view.set(0, "emerald", 1);
    view.data = vec![1, 0, 0];
    h.shot("beacon_l1.png", SIZE, &mut screen, &view, (10.0, 10.0));
    let (l, t) = screen.origin();
    let speed = ((l + 76 - 24 - 1 + 11) as f32, (t + 22 + 11) as f32);
    let out = send(&mut screen, &view, &click(speed.0, speed.1));
    assert!(
        matches!(out.as_slice(), [ScreenAction::ClickSound]),
        "{out:?}"
    );
    let resistance = ((l + 76 - 24 - 1 + 11) as f32, (t + 22 + 25 + 11) as f32);
    assert!(
        send(&mut screen, &view, &click(resistance.0, resistance.1)).is_empty(),
        "tier 2 is locked at level 1"
    );
    let confirm = ((l + 164 + 11) as f32, (t + 107 + 11) as f32);
    let out = send(&mut screen, &view, &click(confirm.0, confirm.1));
    assert!(out.iter().any(|a| matches!(a, ScreenAction::SetBeacon { primary: Some(p), secondary: None } if p == "minecraft:speed")), "{out:?}");
    assert!(
        out.iter()
            .any(|a| matches!(a, ScreenAction::CloseContainer))
    );
}

#[test]
fn beacon_confirm_needs_payment_and_primary() {
    let Some(mut h) = Harness::new() else { return };
    let mut screen = container("beacon", "container.beacon");
    let view = TestView::with_slots(37);
    h.shot("beacon_unpaid.png", SIZE, &mut screen, &view, (10.0, 10.0));
    let (l, t) = screen.origin();
    let confirm = ((l + 164 + 11) as f32, (t + 107 + 11) as f32);
    assert!(send(&mut screen, &view, &click(confirm.0, confirm.1)).is_empty());
    let cancel = ((l + 190 + 11) as f32, (t + 107 + 11) as f32);
    let out = send(&mut screen, &view, &click(cancel.0, cancel.1));
    assert!(
        out.iter()
            .any(|a| matches!(a, ScreenAction::CloseContainer))
    );
}

#[test]
fn drag_preview_shot() {
    let Some(mut h) = Harness::new() else { return };
    let mut screen = ContainerScreen::new(
        MenuKind::Generic { rows: 3 },
        Component::translatable("container.chest", Vec::new()),
    );
    let mut view = TestView::with_slots(63);
    view.set(4, "cobblestone", 20);
    view.carried = Some(stack("cobblestone", 30));
    screen.init(427, 240);
    let pos = |slot: usize| {
        let (l, t) = screen.origin();
        let s = screen.layout().slots[slot];
        ((l + s.x + 8) as f32, (t + s.y + 8) as f32)
    };
    let (a, b, c) = (pos(3), pos(4), pos(12));
    send(
        &mut screen,
        &view,
        &[
            UiInput::press(a.0, a.1, MouseButton::Left),
            UiInput::MouseMove {
                x: a.0 + 1.0,
                y: a.1,
            },
            UiInput::MouseMove { x: b.0, y: b.1 },
            UiInput::MouseMove { x: c.0, y: c.1 },
        ],
    );
    h.shot_inited("screen_drag.png", SIZE, &mut screen, &view, (c.0, c.1));
}
