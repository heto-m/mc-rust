mod common;

use mcr_protocol::types::text::{Component, Content, Style as ProtoStyle};
use mcr_ui::{ChatHud, ChatOptions, DebugOverlay, HeartKind, HudSprites, HudState, UiRenderer};

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
