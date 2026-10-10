mod common;

use mcr_buf::VarInt;
use mcr_data::v26_3;
use mcr_protocol::types::component::{
    DataComponent, EnchantmentLevel, ItemEnchantments, PotionContents,
};
use mcr_protocol::types::item::ItemStack;
use mcr_ui::{HudItems, HudSprites, HudState, ItemIcons, StackView, UiRenderer, vanilla_resources};

fn stack(name: &str, count: i32, components: Vec<DataComponent>) -> ItemStack {
    let id = v26_3()
        .item_by_name(&format!("minecraft:{name}"))
        .unwrap_or_else(|| panic!("unknown item {name}")) as i32;
    let mut s = ItemStack::new(id, count);
    s.components.added = components;
    s
}

fn sample_stacks() -> Vec<ItemStack> {
    vec![
        stack(
            "diamond_sword",
            1,
            vec![DataComponent::Enchantments(ItemEnchantments(vec![
                EnchantmentLevel {
                    enchantment: 0,
                    level: 3,
                },
            ]))],
        ),
        stack("grass_block", 1, vec![]),
        stack("oak_stairs", 1, vec![]),
        stack(
            "potion",
            1,
            vec![DataComponent::PotionContents(PotionContents {
                potion: None,
                custom_color: Some(0xF82423),
                custom_effects: vec![],
                custom_name: None,
            })],
        ),
        stack(
            "leather_helmet",
            1,
            vec![DataComponent::DyedColor(0x1E90FF)],
        ),
        stack("cobblestone", 64, vec![]),
        stack("iron_pickaxe", 1, vec![DataComponent::Damage(VarInt(150))]),
        stack("bow", 1, vec![]),
        stack("clock", 1, vec![]),
        stack("chest", 1, vec![]),
    ]
}

fn render_hotbar(name: &str, size: (u32, u32), millis: u64) -> Option<(Vec<u8>, usize)> {
    let jar = common::jar_path()?;
    let gpu = common::gpu()?;
    let resources = vanilla_resources(&jar, None).expect("resources");
    let mut ui = mcr_ui::Ui::load(&resources).expect("ui");
    let mut icons = ItemIcons::load(&resources).expect("icons");
    icons.set_time(millis);
    let mut renderer = UiRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    let hud = HudSprites::new(&ui);
    let stacks = sample_stacks();
    let views: Vec<StackView> = stacks
        .iter()
        .map(|s| StackView::new(s).expect("valid stack"))
        .collect();
    let mut items = HudItems::default();
    for (slot, view) in views.iter().take(9).enumerate() {
        items.hotbar[slot] = Some(view.view(0.0));
    }
    items.offhand = Some(views[9].view(0.0));
    {
        let mut frame = ui.frame(size.0, size.1);
        let state = HudState {
            selected_slot: 1,
            offhand: true,
            ..HudState::default()
        };
        hud.draw(&mut frame, &state);
        hud.draw_items(&mut frame, &mut icons, &items);
    }
    let img = common::render(
        &gpu,
        &mut renderer,
        &mut ui,
        size,
        wgpu::Color {
            r: 0.42,
            g: 0.55,
            b: 0.36,
            a: 1.0,
        },
    );
    common::save_png(name, size.0, size.1, &img);
    Some((img, icons.stats().rasterized))
}

#[test]
fn hotbar_items_render() {
    let Some((img, rasterized)) = render_hotbar("hotbar_items.png", (854, 480), 1234) else {
        return;
    };
    assert_eq!(rasterized, 10);
    let clear = [107u8, 140, 92];
    let differs = img
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[0].abs_diff(clear[0]) > 3)
        .count();
    assert!(differs > 5000);
}

#[test]
fn glint_animates_only_enchanted_items() {
    let (Some((a, _)), Some((b, _))) = (
        render_hotbar("glint_a.png", (854, 480), 0),
        render_hotbar("glint_b.png", (854, 480), 9000),
    ) else {
        return;
    };
    let slot_diff = |slot: u32| {
        let x0 = (213 - 90 + 20 * slot + 2) * 2;
        let mut n = 0;
        for y in 442..474 {
            for x in x0..x0 + 32 {
                n += usize::from(common::pixel(&a, 854, x, y) != common::pixel(&b, 854, x, y));
            }
        }
        n
    };
    assert!(slot_diff(0) > 5, "sword glint did not animate");
    assert_eq!(slot_diff(2), 0, "stairs must not animate");
}

#[test]
fn repeated_item_is_rasterised_once() {
    let (Some(jar), Some(_)) = (common::jar_path(), common::gpu()) else {
        return;
    };
    let resources = vanilla_resources(&jar, None).expect("resources");
    let mut ui = mcr_ui::Ui::load(&resources).expect("ui");
    let mut icons = ItemIcons::load(&resources).expect("icons");
    let s = stack("grass_block", 1, vec![]);
    let view = StackView::new(&s).unwrap();
    let mut frame = ui.frame(854, 480);
    for i in 0..36 {
        icons.draw(&mut frame, &view.view(0.0), (i % 9) * 20, (i / 9) * 20);
    }
    assert_eq!(icons.stats().rasterized, 1);
    assert_eq!(icons.stats().cached, 35);
}

#[test]
fn hotbar_items_render_at_gui_scale_four() {
    let _ = render_hotbar("hotbar_items_x4.png", (1920, 1080), 777);
}
