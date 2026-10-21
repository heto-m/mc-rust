mod common;

use mcr_protocol::types::text::{Component, Content, Style as ProtoStyle};
use mcr_ui::{
    AirState, ChatHud, ChatOptions, ContextualBar, CrosshairIndicator, DebugOverlay, HeartKind,
    HudEffect, HudSprites, HudState, ScreenAssets, UiRenderer, vanilla_resources,
};

const SIZE: (u32, u32) = (854, 480);
const WORLD: wgpu::Color = wgpu::Color {
    r: 0.42,
    g: 0.55,
    b: 0.36,
    a: 1.0,
};

fn setup() -> Option<(common::Gpu, UiRenderer, mcr_ui::Ui)> {
    let ui = common::ui()?;
    let gpu = common::gpu()?;
    let renderer = UiRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    Some((gpu, renderer, ui))
}

fn differs_from_clear(rgba: &[u8]) -> usize {
    let clear = [107u8, 140, 92];
    rgba.as_chunks::<4>()
        .0
        .iter()
        .filter(|p| {
            p[0].abs_diff(clear[0]) > 3
                || p[1].abs_diff(clear[1]) > 3
                || p[2].abs_diff(clear[2]) > 3
        })
        .count()
}

#[test]
fn fills_blend_premultiplied_and_respect_scale_and_scissor() {
    let Some((gpu, mut renderer, mut ui)) = setup() else {
        return;
    };
    {
        let mut f = ui.frame(640, 480);
        assert_eq!((f.scale(), f.gui_size()), (2, (320, 240)));
        f.fill_rect(10, 10, 20, 20, 0xFFFF_0000);
        f.fill_rect(30, 10, 40, 20, 0x80FF_FFFF);
        f.push_scissor(50, 10, 55, 20);
        f.fill_rect(45, 10, 70, 20, 0xFF00_00FF);
        f.pop_scissor();
    }
    let img = common::render(&gpu, &mut renderer, &mut ui, (640, 480), wgpu::Color::BLACK);
    assert_eq!(common::pixel(&img, 640, 25, 25), [255, 0, 0, 255]);
    assert_eq!(common::pixel(&img, 640, 19, 25), [0, 0, 0, 255]);
    let half = common::pixel(&img, 640, 70, 25);
    assert!(half[0].abs_diff(128) <= 1 && half[0] == half[1] && half[1] == half[2]);
    assert_eq!(common::pixel(&img, 640, 101, 25), [0, 0, 255, 255]);
    assert_eq!(common::pixel(&img, 640, 89, 25), [0, 0, 0, 255]);
    assert_eq!(common::pixel(&img, 640, 111, 25), [0, 0, 0, 255]);
    assert_eq!(renderer.stats().draw_calls, 2);
}

#[test]
fn debug_overlay_renders() {
    let Some((gpu, mut renderer, mut ui)) = setup() else {
        return;
    };
    let overlay = DebugOverlay {
        left: vec![
            "Minecraft 26.3 (mc-rust/vanilla)".into(),
            "60 fps T: 120 vsync fancy-clouds".into(),
            "".into(),
            "XYZ: 123.456 / 64.00000 / -78.901".into(),
            "Block: 123 64 -79".into(),
            "Chunk: 11 0 -5 [7 4 0 in r.0.-1.mca]".into(),
            "Facing: north (Towards negative Z) (180.0 / 0.0)".into(),
            "Client Light: 15 (15 sky, 0 block)".into(),
            "Biome: minecraft:plains".into(),
        ],
        right: vec![
            "Java: 25 64bit".into(),
            "Mem: 36% 1480/4096MB".into(),
            "".into(),
            "Display: 854x480 (NVIDIA)".into(),
            "Vulkan 1.3 (wgpu)".into(),
            "§aGreen §rand §lbold§r §nunder§r §mstrike§r §oitalic".into(),
        ],
        charts: vec![mcr_ui::DebugChart {
            title: "Frame time".into(),
            unit: "ms",
            samples: (0..120).map(|i| 8.0 + (i % 30) as f32).collect(),
            scale: 50.0,
            warn: 16.7,
            bad: 33.4,
        }],
        lightmap: None,
        pie: None,
    };
    {
        let mut f = ui.frame(SIZE.0, SIZE.1);
        overlay.draw(&mut f);
    }
    let img = common::render(&gpu, &mut renderer, &mut ui, SIZE, WORLD);
    let path = common::save_png("debug_overlay.png", SIZE.0, SIZE.1, &img);
    eprintln!("wrote {}", path.display());
    assert!(differs_from_clear(&img) > 2000);
    assert!(renderer.stats().draw_calls <= 3, "{:?}", renderer.stats());
}

#[test]
fn chat_hud_renders_with_fade() {
    let Some((gpu, mut renderer, mut ui)) = setup() else {
        return;
    };
    let options = ChatOptions::default();
    let mut chat = ChatHud::default();
    let say = |name: &str, msg: &str| {
        Component::from_content(Content::Translatable {
            key: "chat.type.text".into(),
            fallback: None,
            args: vec![
                mcr_protocol::types::text::Arg::Component(Component::text(name).styled(
                    ProtoStyle {
                        insertion: Some(name.into()),
                        ..Default::default()
                    },
                )),
                mcr_protocol::types::text::Arg::Str(msg.into()),
            ],
        })
    };
    chat.add(&mut ui, &say("Steve", "hello world"), 0, &options);
    chat.add(&mut ui, &say("Alex", "the quick brown fox jumps over the lazy dog, again and again and again until this line must wrap"), 60, &options);
    chat.add_text(&mut ui, "§eAlex joined the game", 120, &options);
    chat.add_text(
        &mut ui,
        "§c§lWarning:§r §7this message is old",
        10,
        &options,
    );
    {
        let mut f = ui.frame(SIZE.0, SIZE.1);
        chat.draw(&mut f, 150, &options);
    }
    let img = common::render(&gpu, &mut renderer, &mut ui, SIZE, WORLD);
    let path = common::save_png("chat.png", SIZE.0, SIZE.1, &img);
    eprintln!("wrote {}", path.display());
    assert!(differs_from_clear(&img) > 2000);

    {
        let mut f = ui.frame(SIZE.0, SIZE.1);
        chat.draw(&mut f, 400, &options);
    }
    let faded = common::render(&gpu, &mut renderer, &mut ui, SIZE, WORLD);
    assert_eq!(differs_from_clear(&faded), 0);
}

#[test]
fn hotbar_hud_renders() {
    let Some((gpu, mut renderer, mut ui)) = setup() else {
        return;
    };
    let sprites = HudSprites::new(&ui);
    let state = HudState {
        selected_slot: 3,
        offhand: true,
        health: 13,
        displayed_health: 15,
        max_health: 20,
        absorption: 4,
        blink: true,
        armor: 17,
        food: 9,
        xp_progress: 0.6,
        xp_level: 27,
        ..HudState::default()
    };
    {
        let mut f = ui.frame(SIZE.0, SIZE.1);
        sprites.draw(&mut f, &state);
    }
    let img = common::render(&gpu, &mut renderer, &mut ui, SIZE, WORLD);
    let path = common::save_png("hotbar.png", SIZE.0, SIZE.1, &img);
    eprintln!("wrote {}", path.display());
    assert!(differs_from_clear(&img) > 2000);

    let hardcore = HudState {
        hardcore: true,
        heart_kind: HeartKind::Poisoned,
        hunger_effect: true,
        health: 20,
        displayed_health: 20,
        max_health: 40,
        ..HudState::default()
    };
    {
        let mut f = ui.frame(1920, 1080);
        sprites.draw(&mut f, &hardcore);
    }
    let img = common::render(&gpu, &mut renderer, &mut ui, (1920, 1080), WORLD);
    common::save_png("hotbar_hardcore_1080p.png", 1920, 1080, &img);
}

#[test]
fn text_effects_render() {
    let Some((gpu, mut renderer, mut ui)) = setup() else {
        return;
    };
    {
        let mut f = ui.frame(640, 480);
        let lines = [
            "Plain Hello, World! 0123456789",
            "§lBold§r §oItalic§r §nUnderline§r §mStrike§r §kobfuscated§r",
            "§0a§1b§2c§3d§4e§5f§6g§7h§8i§9j§ak§bl§cm§dn§eo§fp",
            "ÀÉÎÕÜ äöüß ¡¿ αβγ ДЖ €",
        ];
        for (i, line) in lines.iter().enumerate() {
            f.text(line, 4.0, 4.0 + 12.0 * i as f32, 0xFFFF_FFFF, true);
        }
        f.text("no shadow", 4.0, 60.0, 0xFFFF_FFFF, false);
        let red = Component::text("styled shadow").styled(ProtoStyle {
            color: Some(mcr_protocol::types::text::Color::Rgb(0xFFAA00)),
            shadow_color: Some(0xFF0000FFu32 as i32),
            ..Default::default()
        });
        f.component(&red, 4.0, 72.0, 0xFFFF_FFFF, true);
    }
    let img = common::render(&gpu, &mut renderer, &mut ui, (640, 480), WORLD);
    common::save_png("text_effects.png", 640, 480, &img);
    assert!(differs_from_clear(&img) > 500);
}

/// 640x480 at GUI scale 2: a 320x240 GUI whose hotbar is centred on x = 160.
const HUD_SIZE: (u32, u32) = (640, 480);

fn hud_shot(
    gpu: &common::Gpu,
    renderer: &mut UiRenderer,
    ui: &mut mcr_ui::Ui,
    draw: impl FnOnce(&mut mcr_ui::UiFrame<'_>),
) -> Vec<u8> {
    {
        let mut f = ui.frame(HUD_SIZE.0, HUD_SIZE.1);
        assert_eq!(f.gui_size(), (320, 240));
        draw(&mut f);
    }
    common::render(gpu, renderer, ui, HUD_SIZE, WORLD)
}

/// Pixels in the GUI rectangle that differ from the clear colour.
fn changed_in(img: &[u8], (x0, y0, x1, y1): (i32, i32, i32, i32)) -> usize {
    let clear = [107u8, 140, 92];
    let mut n = 0;
    for y in (y0 * 2)..(y1 * 2) {
        for x in (x0 * 2)..(x1 * 2) {
            let p = common::pixel(img, HUD_SIZE.0, x as u32, y as u32);
            if (0..3).any(|c| p[c].abs_diff(clear[c]) > 3) {
                n += 1;
            }
        }
    }
    n
}

#[test]
fn effect_icons_fill_a_beneficial_and_a_harmful_row() {
    let Some((gpu, mut renderer, mut ui)) = setup() else {
        return;
    };
    let Some(jar) = common::jar_path() else {
        return;
    };
    let resources = vanilla_resources(&jar, None).expect("resources");
    let assets = ScreenAssets::load(&mut ui, &resources).expect("assets");
    let sprites = HudSprites::new(&ui);
    let speed = assets.texture("mob_effect/speed");
    let poison = assets.texture("mob_effect/poison");
    assert!(speed.is_some() && poison.is_some());
    let effect = |sprite, beneficial, ambient| HudEffect {
        sprite,
        beneficial,
        ambient,
        duration: 600,
    };
    let effects = [
        effect(speed, true, false),
        effect(speed, true, true),
        effect(poison, false, false),
    ];
    let img = hud_shot(&gpu, &mut renderer, &mut ui, |f| {
        sprites.draw_effects(f, &effects)
    });
    common::save_png("hud_effects.png", HUD_SIZE.0, HUD_SIZE.1, &img);
    assert!(
        changed_in(&img, (295, 1, 319, 25)) > 1000,
        "first beneficial icon"
    );
    assert!(
        changed_in(&img, (270, 1, 294, 25)) > 1000,
        "second beneficial icon"
    );
    assert!(changed_in(&img, (295, 27, 319, 51)) > 1000, "harmful row");
    assert_eq!(
        changed_in(&img, (270, 27, 294, 51)),
        0,
        "one harmful icon only"
    );
    assert_eq!(changed_in(&img, (0, 60, 320, 240)), 0);
}

#[test]
fn attack_indicator_modes() {
    let Some((gpu, mut renderer, mut ui)) = setup() else {
        return;
    };
    let sprites = HudSprites::new(&ui);
    let img = hud_shot(&gpu, &mut renderer, &mut ui, |f| {
        sprites.draw_crosshair(f, CrosshairIndicator::Progress(0.5))
    });
    common::save_png("hud_crosshair_progress.png", HUD_SIZE.0, HUD_SIZE.1, &img);
    let inverted = [255 - 107, 255 - 140, 255 - 92];
    let inverted_pixels = (112 * 2..127 * 2)
        .flat_map(|y| (152 * 2..167 * 2).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let p = common::pixel(&img, HUD_SIZE.0, x, y);
            (0..3).all(|c| p[c].abs_diff(inverted[c]) <= 3)
        })
        .count();
    assert!(
        inverted_pixels > 20,
        "the crosshair inverts what is under it"
    );
    let bar = (152, 129, 168, 133);
    assert!(
        changed_in(&img, bar) > 0,
        "charging bar under the crosshair"
    );
    let full = hud_shot(&gpu, &mut renderer, &mut ui, |f| {
        sprites.draw_crosshair(f, CrosshairIndicator::Full)
    });
    assert!(changed_in(&full, (152, 129, 168, 145)) > changed_in(&img, (152, 129, 168, 145)));
    let none = hud_shot(&gpu, &mut renderer, &mut ui, |f| {
        sprites.draw_crosshair(f, CrosshairIndicator::None)
    });
    assert_eq!(changed_in(&none, (152, 129, 168, 145)), 0);

    let hotbar = |left_handed| HudState {
        hotbar_attack: Some(0.5),
        left_handed,
        player_health: false,
        contextual: ContextualBar::Empty,
        ..HudState::default()
    };
    let right = hud_shot(&gpu, &mut renderer, &mut ui, |f| {
        sprites.draw(f, &hotbar(false))
    });
    common::save_png("hud_hotbar_attack.png", HUD_SIZE.0, HUD_SIZE.1, &right);
    assert!(
        changed_in(&right, (257, 220, 275, 238)) > 100,
        "indicator right of the hotbar"
    );
    assert_eq!(changed_in(&right, (47, 220, 65, 238)), 0);
    let left = hud_shot(&gpu, &mut renderer, &mut ui, |f| {
        sprites.draw(f, &hotbar(true))
    });
    assert!(
        changed_in(&left, (47, 220, 65, 238)) > 100,
        "left-handed: indicator on the left"
    );
    assert_eq!(changed_in(&left, (257, 220, 275, 238)), 0);
}

#[test]
fn air_bubbles_show_under_water_and_while_recovering() {
    let Some((gpu, mut renderer, mut ui)) = setup() else {
        return;
    };
    let sprites = HudSprites::new(&ui);
    let row = (170, 191, 251, 201);
    let with_air = |air| HudState {
        air,
        ..HudState::default()
    };
    let dry = hud_shot(&gpu, &mut renderer, &mut ui, |f| {
        sprites.draw(f, &with_air(AirState::default()))
    });
    assert_eq!(
        changed_in(&dry, row),
        0,
        "full air out of water hides the row"
    );
    let diving = hud_shot(&gpu, &mut renderer, &mut ui, |f| {
        sprites.draw(
            f,
            &with_air(AirState {
                underwater: true,
                ..AirState::default()
            }),
        )
    });
    common::save_png("hud_air_full.png", HUD_SIZE.0, HUD_SIZE.1, &diving);
    let full = changed_in(&diving, row);
    assert!(full > 500, "ten full bubbles under water");
    let low = hud_shot(&gpu, &mut renderer, &mut ui, |f| {
        sprites.draw(
            f,
            &with_air(AirState {
                supply: 31,
                max: 300,
                underwater: true,
            }),
        )
    });
    common::save_png("hud_air_low.png", HUD_SIZE.0, HUD_SIZE.1, &low);
    assert!(changed_in(&low, row) > 200, "empty bubbles are drawn too");
    assert!(
        changed_in(&low, (243, 191, 251, 201)) > 20,
        "rightmost bubble"
    );
    assert!(
        changed_in(&low, (171, 191, 179, 201)) > 20,
        "leftmost empty bubble"
    );
}

#[test]
fn left_handed_offhand_slot_sits_right_of_the_hotbar() {
    let Some((gpu, mut renderer, mut ui)) = setup() else {
        return;
    };
    let sprites = HudSprites::new(&ui);
    let state = |left_handed| HudState {
        offhand: true,
        left_handed,
        player_health: false,
        contextual: ContextualBar::Empty,
        ..HudState::default()
    };
    let (left_slot, right_slot) = ((40, 217, 67, 240), (253, 217, 280, 240));
    let right = hud_shot(&gpu, &mut renderer, &mut ui, |f| {
        sprites.draw(f, &state(false))
    });
    assert!(changed_in(&right, left_slot) > 500);
    assert_eq!(changed_in(&right, right_slot), 0);
    let left = hud_shot(&gpu, &mut renderer, &mut ui, |f| {
        sprites.draw(f, &state(true))
    });
    common::save_png("hud_offhand_right.png", HUD_SIZE.0, HUD_SIZE.1, &left);
    assert!(changed_in(&left, right_slot) > 500);
    assert_eq!(changed_in(&left, left_slot), 0);
}

#[test]
fn vehicle_hearts_replace_food_and_jump_bar_replaces_xp() {
    let Some((gpu, mut renderer, mut ui)) = setup() else {
        return;
    };
    let sprites = HudSprites::new(&ui);
    let riding = |scale| HudState {
        vehicle_hearts: 15,
        vehicle_health: 21,
        contextual: ContextualBar::Jump {
            scale,
            cooldown: false,
        },
        ..HudState::default()
    };
    let img = hud_shot(&gpu, &mut renderer, &mut ui, |f| {
        sprites.draw(f, &riding(0.0))
    });
    common::save_png("hud_vehicle.png", HUD_SIZE.0, HUD_SIZE.1, &img);
    let first_row = (170, 201, 251, 210);
    let second_row = (210, 191, 251, 200);
    assert!(
        changed_in(&img, first_row) > 500,
        "ten mount hearts on the food row"
    );
    assert!(changed_in(&img, second_row) > 200, "five more above");
    assert_eq!(
        changed_in(&img, (170, 191, 209, 200)),
        0,
        "second row holds five"
    );
    let charged = hud_shot(&gpu, &mut renderer, &mut ui, |f| {
        sprites.draw(f, &riding(1.0))
    });
    let bar = |img: &[u8], x0| {
        let mut v = Vec::new();
        for x in (x0 * 2)..((x0 + 20) * 2) {
            v.push(common::pixel(img, HUD_SIZE.0, x as u32, (240 - 27) * 2));
        }
        v
    };
    assert_ne!(
        bar(&img, 120),
        bar(&charged, 120),
        "the jump charge fills the bar"
    );
    let none = hud_shot(&gpu, &mut renderer, &mut ui, |f| {
        sprites.draw(
            f,
            &HudState {
                contextual: ContextualBar::Empty,
                ..HudState::default()
            },
        )
    });
    assert_eq!(
        changed_in(&none, (69, 211, 251, 216)),
        0,
        "no bar in the empty state"
    );
}

#[test]
fn linear_sprites_blend_between_texels() {
    let Some((gpu, mut renderer, mut ui)) = setup() else {
        return;
    };
    let px = [0, 0, 0, 255, 255, 255, 255, 255];
    let sprite = ui.sprites_mut().add_raw(2, 1, &px).expect("fits");
    let draw = |ui: &mut mcr_ui::Ui, linear: bool| {
        let mut f = ui.frame(200, 100);
        f.set_linear(linear);
        f.sprite_quad(
            sprite,
            [[0.0, 0.0], [0.0, 100.0], [200.0, 100.0], [200.0, 0.0]],
            [[0.25, 0.5], [0.25, 0.5], [0.75, 0.5], [0.75, 0.5]],
            0xFFFF_FFFF,
        );
    };
    draw(&mut ui, false);
    let nearest = common::render(&gpu, &mut renderer, &mut ui, (200, 100), wgpu::Color::BLACK);
    draw(&mut ui, true);
    let linear = common::render(&gpu, &mut renderer, &mut ui, (200, 100), wgpu::Color::BLACK);
    assert_eq!(common::pixel(&nearest, 200, 60, 50)[0], 0);
    assert_eq!(common::pixel(&nearest, 200, 140, 50)[0], 255);
    let (left, mid, right) = (
        common::pixel(&linear, 200, 60, 50)[0],
        common::pixel(&linear, 200, 100, 50)[0],
        common::pixel(&linear, 200, 140, 50)[0],
    );
    assert!(
        0 < left && left < mid && mid < right && right < 255,
        "{left} {mid} {right}"
    );
}
