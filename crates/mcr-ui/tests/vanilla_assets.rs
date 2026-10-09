mod common;

use mcr_protocol::types::text::{Arg, Component, Content};
use mcr_ui::{FontId, Scaling, parse_legacy};

#[test]
fn default_font_metrics_match_vanilla() {
    let Some(mut ui) = common::ui_with_index(false) else {
        return;
    };
    let fonts = ui.fonts_mut();
    let adv = |f: &mut mcr_ui::Fonts, c| f.advance(FontId::DEFAULT, c, false);
    assert_eq!(adv(fonts, 'i'), 2.0);
    assert_eq!(adv(fonts, '!'), 2.0);
    assert_eq!(adv(fonts, 'l'), 3.0);
    assert_eq!(adv(fonts, 't'), 4.0);
    assert_eq!(adv(fonts, 'f'), 5.0);
    assert_eq!(adv(fonts, 'I'), 4.0);
    assert_eq!(adv(fonts, 'W'), 6.0);
    assert_eq!(adv(fonts, ' '), 4.0);
    assert_eq!(adv(fonts, '\u{200c}'), 0.0);
}

#[test]
fn bold_adds_one_and_missing_glyph_is_six_wide() {
    let Some(mut ui) = common::ui_with_index(false) else {
        return;
    };
    let fonts = ui.fonts_mut();
    assert_eq!(fonts.advance(FontId::DEFAULT, 'i', true), 3.0);
    assert_eq!(fonts.advance(FontId::DEFAULT, '\u{10ffff}', false), 6.0);
}

#[test]
fn section_sign_is_formatting_not_a_glyph() {
    let Some(mut ui) = common::ui_with_index(false) else {
        return;
    };
    assert_eq!(ui.text_width("§aHi"), ui.text_width("Hi"));
    assert_eq!(ui.text_width("Hi§"), ui.text_width("Hi"));
    assert_eq!(ui.text_width("§lHi"), ui.text_width("Hi") + 2.0);
    assert_eq!(parse_legacy("§§a").len(), 1);
}

#[test]
fn alt_and_illager_fonts_load() {
    let Some(mut ui) = common::ui_with_index(false) else {
        return;
    };
    let fonts = ui.fonts_mut();
    for font in [FontId::ALT, FontId::ILLAGERALT] {
        assert!(fonts.advance(font, 'a', false) > 0.0);
        assert_eq!(fonts.advance(font, ' ', false), 4.0);
    }
}

#[test]
fn unihex_glyphs_use_half_pixel_metrics_when_unifont_is_available() {
    let Some(mut ui) = common::ui() else { return };
    if common::asset_index().is_none() {
        eprintln!("skipping: no launcher asset index");
        return;
    }
    let fonts = ui.fonts_mut();
    assert_eq!(fonts.advance(FontId::DEFAULT, '中', false), 9.0);
    let g = fonts.glyph(FontId::DEFAULT, '中');
    assert_eq!(fonts.info(g).shadow_offset, 0.5);
    let baked = fonts.baked(g);
    assert_eq!(
        (baked.right - baked.left, baked.down - baked.up),
        (8.0, 8.0)
    );
    assert!(fonts.advance(FontId::UNIFORM, 'a', false) > 0.0);
}

#[test]
fn chat_translation_resolves_from_the_jar() {
    let Some(ui) = common::ui_with_index(false) else {
        return;
    };
    let say = Component::from_content(Content::Translatable {
        key: "chat.type.text".into(),
        fallback: None,
        args: vec![Arg::Str("Steve".into()), Arg::Str("hi".into())],
    });
    let text: String = ui.flatten(&say).iter().map(|c| c.ch).collect();
    assert_eq!(text, "<Steve> hi");
}

#[test]
fn gui_sprites_load_with_mcmeta_scaling() {
    let Some(ui) = common::ui_with_index(false) else {
        return;
    };
    let sprites = ui.sprites();
    assert!(sprites.len() > 400);
    let hotbar = *sprites.info(sprites.id("hud/hotbar").unwrap());
    assert_eq!((hotbar.width, hotbar.height), (182, 22));
    let button = *sprites.info(sprites.id("widget/button").unwrap());
    assert!(matches!(
        button.scaling,
        Scaling::NineSlice {
            width: 200,
            height: 20,
            ..
        }
    ));
}
