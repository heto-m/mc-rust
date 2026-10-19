//! Vanilla's `OptionsScreen` and the option pages it opens.

use std::rc::Rc;

use mcr_settings::value::{
    AttackIndicator, ChatVisibility, CloudStatus, Fov, GraphicsPreset, HumanoidArm,
    InactivityFpsLimit, MusicFrequency, MusicToastState, ParticleStatus, PreferredGraphicsApi,
    PresenceSharing, PrioritizeChunkUpdates, QuotedString, TextureFiltering,
};
use mcr_settings::{MODEL_PARTS, Options, SOUND_CATEGORIES};
use mcr_ui::{
    Button, ConfirmScreen, CreditsScreen, CreditsSource, Env, Key, LanguageInfo, RenderCx, Screen,
    ScreenAction, Slider, TooltipLines, UiInput, WidgetEventKind, WidgetSet, draw_menu_background,
};

use crate::language::LanguageScreen;
use crate::options_list::{
    FOOTER, FooterButton, Opt, OptionsPage, Row, generic, generic_key, percent, percent_or_off,
    pixels, small, status,
};
use crate::settings::SharedSettings;

const ACCESSIBILITY_HELP: &str = "https://aka.ms/MinecraftJavaAccessibility";
const ATTRIBUTION: &str = "https://aka.ms/MinecraftJavaAttribution";
const LICENSES: &str = "https://aka.ms/MinecraftJavaLicenses";
const PRIVACY_AND_ONLINE_SETTINGS: &str = "https://aka.ms/MinecraftJavaXboxPrivacyAndSafety";
const WHITE: u32 = 0xFFFF_FFFF;

/// What the option pages need from the running client.
pub struct OptionsHost {
    pub settings: SharedSettings,
    pub languages: Vec<LanguageInfo>,
    /// `Window#calculateScale(0, …)`: the largest GUI scale the window fits.
    pub max_gui_scale: i32,
    pub credits: Rc<dyn Fn() -> CreditsSource>,
    pub player: String,
    /// The world has the `minecart_improvements` feature, so Rotate with Minecarts applies.
    pub minecart_improvements: bool,
    pub packs: crate::packs::PackHost,
}

pub type Host = Rc<OptionsHost>;

fn push(screen: impl Screen + 'static) -> Vec<ScreenAction> {
    vec![ScreenAction::Push(Box::new(screen))]
}

fn link(url: &str) -> Vec<ScreenAction> {
    ConfirmScreen::link(url.to_owned())
        .map(|s| ScreenAction::Push(Box::new(s)))
        .into_iter()
        .collect()
}

/// Any change to a quality option leaves the graphics preset as Custom.
fn quality(opt: Opt) -> Opt {
    opt.on_set(|o| o.graphics_preset = GraphicsPreset::Custom)
}

fn fov_label(cx: &mut RenderCx<'_, '_>, fov: i32) -> String {
    match fov {
        70 => generic_key(cx, "options.fov", "options.fov.min"),
        110 => generic_key(cx, "options.fov", "options.fov.max"),
        v => generic(cx, "options.fov", &v.to_string()),
    }
}

fn narrator() -> Opt {
    Opt::cycle_with(
        1,
        |_| 0,
        |_, _| {},
        |_, cx| generic_key(cx, "options.narrator", "options.narrator.notavailable"),
    )
    .inactive()
}

fn sound_slider(i: usize) -> Opt {
    let caption: &'static str = match SOUND_CATEGORIES[i] {
        "master" => "soundCategory.master",
        "music" => "soundCategory.music",
        "record" => "soundCategory.record",
        "weather" => "soundCategory.weather",
        "block" => "soundCategory.block",
        "hostile" => "soundCategory.hostile",
        "neutral" => "soundCategory.neutral",
        "player" => "soundCategory.player",
        "ambient" => "soundCategory.ambient",
        "voice" => "soundCategory.voice",
        _ => "soundCategory.ui",
    };
    Opt::slider(
        move |o| o.sound_volumes[i],
        move |o, v| o.sound_volumes[i] = v,
        move |o, cx| percent_or_off(cx, caption, o.sound_volumes[i]),
    )
}

fn sound_device_label(cx: &mut RenderCx<'_, '_>, device: &str) -> String {
    if device.is_empty() {
        generic_key(cx, "options.audioDevice", "options.audioDevice.default")
    } else {
        let name = device.strip_prefix("OpenAL Soft on ").unwrap_or(device);
        generic(cx, "options.audioDevice", name)
    }
}

fn chat_delay(cx: &mut RenderCx<'_, '_>, seconds: f64) -> String {
    if seconds <= 0.0 {
        cx.lang("options.chat.delay_none")
    } else {
        cx.lang_fmt("options.chat.delay", &[&format!("{seconds:.1}")])
    }
}

fn opacity(caption: &'static str, get: fn(&Options) -> f64, set: fn(&mut Options, f64)) -> Opt {
    Opt::unit(get, set, move |cx, v| percent(cx, caption, v))
}

fn chat_opacity() -> Opt {
    Opt::unit(
        |o| o.chat_opacity,
        |o, v| o.chat_opacity = v,
        |cx, v| percent(cx, "options.chat.opacity", v * 0.9 + 0.1),
    )
}

fn text_background_opacity() -> Opt {
    opacity(
        "options.accessibility.text_background_opacity",
        |o| o.text_background_opacity,
        |o, v| o.text_background_opacity = v,
    )
}

fn line_spacing() -> Opt {
    opacity(
        "options.chat.line_spacing",
        |o| o.chat_line_spacing,
        |o, v| o.chat_line_spacing = v,
    )
}

fn chat_delay_slider() -> Opt {
    Opt::int(
        (0, 60),
        |o| (o.chat_delay * 10.0).round() as i32,
        |o, v| o.chat_delay = f64::from(v) / 10.0,
        |cx, v| chat_delay(cx, f64::from(v) / 10.0),
    )
}

fn show_subtitles() -> Opt {
    Opt::toggle(
        "options.showSubtitles",
        |o| o.show_subtitles,
        |o, v| o.show_subtitles = v,
    )
    .tooltip("options.showSubtitles.tooltip")
}

/// `OptionInstance.UnitDouble.xmap(Mth::square, Math::sqrt)`: the slider shows the square root.
fn squared(
    caption: &'static str,
    tooltip: &'static str,
    get: fn(&Options) -> f64,
    set: fn(&mut Options, f64),
) -> Opt {
    Opt::slider(
        move |o| get(o).max(0.0).sqrt(),
        move |o, v| set(o, v * v),
        move |o, cx| percent_or_off(cx, caption, get(o)),
    )
    .tooltip(tooltip)
}

fn scale(
    caption: &'static str,
    tooltip: &'static str,
    get: fn(&Options) -> f64,
    set: fn(&mut Options, f64),
) -> Opt {
    Opt::unit(get, set, move |cx, v| percent_or_off(cx, caption, v)).tooltip(tooltip)
}

fn toggle_key(
    caption: &'static str,
    get: fn(&Options) -> bool,
    set: fn(&mut Options, bool),
) -> Opt {
    Opt::toggle_labels(caption, get, set, "options.key.toggle", "options.key.hold")
}

pub fn skin(host: &Host) -> OptionsPage {
    let mut opts: Vec<Opt> = (0..MODEL_PARTS.len())
        .map(|i| {
            let caption: &'static str = match MODEL_PARTS[i] {
                "cape" => "options.modelPart.cape",
                "jacket" => "options.modelPart.jacket",
                "left_sleeve" => "options.modelPart.left_sleeve",
                "right_sleeve" => "options.modelPart.right_sleeve",
                "left_pants_leg" => "options.modelPart.left_pants_leg",
                "right_pants_leg" => "options.modelPart.right_pants_leg",
                _ => "options.modelPart.hat",
            };
            Opt::toggle_with(
                caption,
                move |o| o.model_parts[i],
                move |o, v| o.model_parts[i] = v,
            )
        })
        .collect();
    opts.push(Opt::cycle(
        "options.mainHand",
        HumanoidArm::ALL,
        |o| o.main_hand,
        |o, v| o.main_hand = v,
        |v| match v {
            HumanoidArm::Left => "options.mainHand.left",
            HumanoidArm::Right => "options.mainHand.right",
        },
    ));
    OptionsPage::new(
        "options.skinCustomisation.title",
        host.settings.clone(),
        small(opts),
    )
}

pub fn sound(host: &Host) -> OptionsPage {
    let mut rows = vec![Row::Big(sound_slider(0))];
    rows.extend(small(
        (1..SOUND_CATEGORIES.len()).map(sound_slider).collect(),
    ));
    let mut devices = vec![String::new()];
    devices.extend(mcr_audio::output_devices());
    let current = host.settings.borrow().file.options.sound_device.0.clone();
    if !devices.contains(&current) {
        devices.push(current);
    }
    let devices = Rc::new(devices);
    let (get_list, set_list) = (devices.clone(), devices.clone());
    rows.push(Row::Big(Opt::cycle_with(
        devices.len(),
        move |o| {
            get_list
                .iter()
                .position(|d| *d == o.sound_device.0)
                .unwrap_or(0)
        },
        move |o, i| o.sound_device = QuotedString(set_list[i].clone()),
        |o, cx| sound_device_label(cx, &o.sound_device.0),
    )));
    rows.push(Row::Small(
        show_subtitles(),
        Some(
            Opt::toggle(
                "options.directionalAudio",
                |o| o.directional_audio,
                |o, v| o.directional_audio = v,
            )
            .tooltip_with(|o, cx| {
                cx.lang(if o.directional_audio {
                    "options.directionalAudio.on.tooltip"
                } else {
                    "options.directionalAudio.off.tooltip"
                })
            }),
        ),
    ));
    rows.push(Row::Small(
        Opt::cycle(
            "options.music_frequency",
            MusicFrequency::ALL,
            |o| o.music_frequency,
            |o, v| o.music_frequency = v,
            |v| match v {
                MusicFrequency::Default => "options.music_frequency.default",
                MusicFrequency::Frequent => "options.music_frequency.frequent",
                MusicFrequency::Constant => "options.music_frequency.constant",
            },
        )
        .tooltip("options.music_frequency.tooltip"),
        Some(
            Opt::cycle(
                "options.musicToast",
                MusicToastState::ALL,
                |o| o.music_toast,
                |o, v| o.music_toast = v,
                music_toast_key,
            )
            .tooltip_with(|o, cx| cx.lang(&format!("{}.tooltip", music_toast_key(o.music_toast)))),
        ),
    ));
    OptionsPage::new("options.sounds.title", host.settings.clone(), rows)
}

fn music_toast_key(v: MusicToastState) -> &'static str {
    match v {
        MusicToastState::Never => "options.musicToast.never",
        MusicToastState::Pause => "options.musicToast.pauseMenu",
        MusicToastState::PauseAndToast => "options.musicToast.pauseMenuAndToast",
    }
}

pub fn video(host: &Host) -> OptionsPage {
    let max_scale = host.max_gui_scale.max(1);
    let mut rows = vec![Row::Header("options.video.display.header")];
    rows.push(Row::Big(
        Opt::cycle_with(
            1,
            |_| 0,
            |_, _| {},
            |_, cx| {
                generic_key(
                    cx,
                    "options.fullscreen.exclusive.mode",
                    "options.fullscreen.current",
                )
            },
        )
        .tooltip("options.fullscreen.exclusive.mode.tooltip"),
    ));
    rows.extend(small(vec![
        Opt::int(
            (1, 26),
            |o| o.max_fps / 10,
            |o, v| o.max_fps = v * 10,
            |cx, v| {
                if v * 10 >= 260 {
                    generic_key(cx, "options.framerateLimit", "options.framerateLimit.max")
                } else {
                    let fps = cx.lang_fmt("options.framerate", &[&(v * 10).to_string()]);
                    generic(cx, "options.framerateLimit", &fps)
                }
            },
        ),
        Opt::toggle(
            "options.vsync",
            |o| o.enable_vsync,
            |o, v| o.enable_vsync = v,
        ),
        Opt::cycle(
            "options.inactivityFpsLimit",
            InactivityFpsLimit::ALL,
            |o| o.inactivity_fps_limit,
            |o, v| o.inactivity_fps_limit = v,
            |v| match v {
                InactivityFpsLimit::Minimized => "options.inactivityFpsLimit.minimized",
                InactivityFpsLimit::Afk => "options.inactivityFpsLimit.afk",
            },
        )
        .tooltip_with(|o, cx| {
            cx.lang(match o.inactivity_fps_limit {
                InactivityFpsLimit::Minimized => "options.inactivityFpsLimit.minimized.tooltip",
                InactivityFpsLimit::Afk => "options.inactivityFpsLimit.afk.tooltip",
            })
        }),
        Opt::cycle_with(
            max_scale as usize + 1,
            move |o| o.gui_scale.clamp(0, max_scale) as usize,
            |o, i| o.gui_scale = i as i32,
            |o, cx| {
                if o.gui_scale == 0 {
                    generic_key(cx, "options.guiScale", "options.guiScale.auto")
                } else {
                    generic(cx, "options.guiScale", &o.gui_scale.to_string())
                }
            },
        ),
        Opt::toggle(
            "options.fullscreen",
            |o| o.fullscreen,
            |o, v| o.fullscreen = v,
        ),
        Opt::toggle(
            "options.exclusiveFullscreen",
            |o| o.exclusive_fullscreen,
            |o, v| o.exclusive_fullscreen = v,
        )
        .tooltip_with(|o, cx| {
            cx.lang(if o.exclusive_fullscreen {
                "options.exclusiveFullscreen.on.tooltip"
            } else {
                "options.exclusiveFullscreen.off.tooltip"
            })
        }),
        Opt::unit(
            |o| o.gamma,
            |o, v| o.gamma = v,
            |cx, v| match (v * 100.0) as i32 {
                0 => generic_key(cx, "options.gamma", "options.gamma.min"),
                50 => generic_key(cx, "options.gamma", "options.gamma.default"),
                100 => generic_key(cx, "options.gamma", "options.gamma.max"),
                p => generic(cx, "options.gamma", &p.to_string()),
            },
        ),
        Opt::cycle(
            "options.graphicsApi",
            PreferredGraphicsApi::ALL,
            |o| o.preferred_graphics_backend,
            |o, v| o.preferred_graphics_backend = v,
            |v| match v {
                PreferredGraphicsApi::Default => "options.graphicsApi.default",
                PreferredGraphicsApi::OpenGl => "options.graphicsApi.opengl",
                PreferredGraphicsApi::Vulkan => "options.graphicsApi.vulkan",
            },
        )
        .tooltip("options.graphicsApi.tooltip"),
    ]));
    rows.push(Row::Header("options.video.quality.header"));
    rows.push(Row::Big(
        Opt::int(
            (0, GraphicsPreset::ALL.len() as i32 - 1),
            |o| {
                GraphicsPreset::ALL
                    .iter()
                    .position(|p| *p == o.graphics_preset)
                    .unwrap_or(0) as i32
            },
            |o, i| o.apply_graphics_preset(GraphicsPreset::ALL[i.clamp(0, 3) as usize]),
            |cx, i| {
                let key = match GraphicsPreset::ALL[i.clamp(0, 3) as usize] {
                    GraphicsPreset::Fast => "options.graphics.fast",
                    GraphicsPreset::Fancy => "options.graphics.fancy",
                    GraphicsPreset::Fabulous => "options.graphics.fabulous",
                    GraphicsPreset::Custom => "options.graphics.custom",
                };
                generic_key(cx, "options.graphics.preset", key)
            },
        )
        .deferred()
        .tooltip("options.graphics.preset.tooltip"),
    ));
    let chunks = |caption: &'static str| {
        move |cx: &mut RenderCx<'_, '_>, v: i32| {
            let n = cx.lang_fmt("options.chunks", &[&v.to_string()]);
            generic(cx, caption, &n)
        }
    };
    rows.extend(small(
        vec![
            Opt::int(
                (0, 7),
                |o| o.biome_blend_radius,
                |o, v| o.biome_blend_radius = v,
                |cx, v| {
                    generic_key(
                        cx,
                        "options.biomeBlendRadius",
                        &format!("options.biomeBlendRadius.{}", v * 2 + 1),
                    )
                },
            )
            .deferred(),
            Opt::int(
                (2, 32),
                |o| o.render_distance,
                |o, v| o.render_distance = v,
                chunks("options.renderDistance"),
            )
            .deferred(),
            Opt::cycle(
                "options.prioritizeChunkUpdates",
                PrioritizeChunkUpdates::ALL,
                |o| o.prioritize_chunk_updates,
                |o, v| o.prioritize_chunk_updates = v,
                |v| match v {
                    PrioritizeChunkUpdates::None => "options.prioritizeChunkUpdates.none",
                    PrioritizeChunkUpdates::PlayerAffected => {
                        "options.prioritizeChunkUpdates.byPlayer"
                    }
                    PrioritizeChunkUpdates::Nearby => "options.prioritizeChunkUpdates.nearby",
                },
            )
            .tooltip_with(|o, cx| {
                cx.lang(match o.prioritize_chunk_updates {
                    PrioritizeChunkUpdates::None => "options.prioritizeChunkUpdates.none.tooltip",
                    PrioritizeChunkUpdates::PlayerAffected => {
                        "options.prioritizeChunkUpdates.byPlayer.tooltip"
                    }
                    PrioritizeChunkUpdates::Nearby => {
                        "options.prioritizeChunkUpdates.nearby.tooltip"
                    }
                })
            }),
            Opt::int(
                (5, 32),
                |o| o.simulation_distance,
                |o, v| o.simulation_distance = v,
                chunks("options.simulationDistance"),
            )
            .deferred(),
            Opt::toggle(
                "options.ao",
                |o| o.ambient_occlusion,
                |o, v| o.ambient_occlusion = v,
            ),
            Opt::cycle(
                "options.renderClouds",
                CloudStatus::ALL,
                |o| o.render_clouds,
                |o, v| o.render_clouds = v,
                |v| match v {
                    CloudStatus::Off => "options.off",
                    CloudStatus::Fast => "options.clouds.fast",
                    CloudStatus::Fancy => "options.clouds.fancy",
                },
            ),
            Opt::cycle(
                "options.particles",
                ParticleStatus::ALL,
                |o| o.particles,
                |o, v| o.particles = v,
                |v| match v {
                    ParticleStatus::All => "options.particles.all",
                    ParticleStatus::Decreased => "options.particles.decreased",
                    ParticleStatus::Minimal => "options.particles.minimal",
                },
            ),
            Opt::int(
                (0, 4),
                |o| o.mipmap_levels,
                |o, v| o.mipmap_levels = v,
                |cx, v| {
                    if v == 0 {
                        status(cx, "options.mipmapLevels", false)
                    } else {
                        generic(cx, "options.mipmapLevels", &v.to_string())
                    }
                },
            )
            .deferred(),
            Opt::toggle(
                "options.entityShadows",
                |o| o.entity_shadows,
                |o, v| o.entity_shadows = v,
            ),
            Opt::int(
                (2, 20),
                |o| (o.entity_distance_scaling * 4.0).round() as i32,
                |o, v| o.entity_distance_scaling = f64::from(v) / 4.0,
                |cx, v| percent(cx, "options.entityDistanceScaling", f64::from(v) / 4.0),
            ),
            Opt::int(
                (0, 10),
                |o| o.menu_background_blurriness,
                |o, v| o.menu_background_blurriness = v,
                |cx, v| {
                    let caption = "options.accessibility.menu_background_blurriness";
                    if v == 0 {
                        generic_key(cx, caption, "options.off")
                    } else {
                        generic(cx, caption, &v.to_string())
                    }
                },
            )
            .tooltip("options.accessibility.menu_background_blurriness.tooltip"),
            Opt::int(
                (2, 128),
                |o| o.cloud_range,
                |o, v| o.cloud_range = v,
                chunks("options.renderCloudsDistance"),
            ),
            Opt::toggle(
                "options.cutoutLeaves",
                |o| o.cutout_leaves,
                |o, v| o.cutout_leaves = v,
            )
            .tooltip("options.cutoutLeaves.tooltip"),
            Opt::toggle(
                "options.improvedTransparency",
                |o| o.improved_transparency,
                |o, v| o.improved_transparency = v,
            )
            .tooltip("options.improvedTransparency.oit.tooltip"),
            Opt::cycle(
                "options.textureFiltering",
                TextureFiltering::ALL,
                |o| o.texture_filtering,
                |o, v| o.texture_filtering = v,
                |v| match v {
                    TextureFiltering::None => "options.textureFiltering.none",
                    TextureFiltering::Rgss => "options.textureFiltering.rgss",
                    TextureFiltering::Anisotropic => "options.textureFiltering.anisotropic",
                },
            )
            .tooltip_with(|o, cx| {
                cx.lang(match o.texture_filtering {
                    TextureFiltering::None => "options.textureFiltering.none.tooltip",
                    TextureFiltering::Rgss => "options.textureFiltering.rgss.tooltip",
                    TextureFiltering::Anisotropic => "options.textureFiltering.anisotropic.tooltip",
                })
            }),
            Opt::int(
                (1, 3),
                |o| o.max_anisotropy_bit,
                |o, v| o.max_anisotropy_bit = v,
                |cx, v| {
                    let factor = cx.lang_fmt("options.multiplier", &[&(1 << v).to_string()]);
                    generic(cx, "options.maxAnisotropy", &factor)
                },
            )
            .active_if(|o| o.texture_filtering == TextureFiltering::Anisotropic)
            .tooltip("options.maxAnisotropy.tooltip"),
            Opt::int(
                (3, 10),
                |o| o.weather_radius,
                |o, v| o.weather_radius = v,
                |cx, v| {
                    let n = cx.lang_fmt("options.blocks", &[&v.to_string()]);
                    generic(cx, "options.weatherRadius", &n)
                },
            )
            .tooltip("options.weatherRadius.tooltip"),
        ]
        .into_iter()
        .map(quality)
        .collect(),
    ));
    rows.push(Row::Header("options.video.preferences.header"));
    let mut preferences = vec![
        Opt::toggle(
            "options.autosaveIndicator",
            |o| o.show_autosave_indicator,
            |o, v| o.show_autosave_indicator = v,
        ),
        Opt::toggle("options.vignette", |o| o.vignette, |o, v| o.vignette = v)
            .tooltip("options.vignette.tooltip"),
        Opt::cycle(
            "options.attackIndicator",
            AttackIndicator::ALL,
            |o| o.attack_indicator,
            |o, v| o.attack_indicator = v,
            |v| match v {
                AttackIndicator::Off => "options.off",
                AttackIndicator::Crosshair => "options.attack.crosshair",
                AttackIndicator::Hotbar => "options.attack.hotbar",
            },
        ),
        Opt::int(
            (0, 40),
            |o| (o.chunk_section_fade_in_time * 20.0).round() as i32,
            |o, v| o.chunk_section_fade_in_time = f64::from(v) / 20.0,
            |cx, v| {
                if v <= 0 {
                    cx.lang("options.chunkFade.none")
                } else {
                    let s = format!("{:.2}", f64::from(v) / 20.0);
                    cx.lang_fmt("options.chunkFade.seconds", &[&s])
                }
            },
        )
        .tooltip("options.chunkFade.tooltip"),
    ];
    if host.settings.borrow().dynamic_lights_provider {
        preferences.push(Opt::toggle(
            "Mod Dynamic Lights",
            |o| o.mcr_dynamic_lights,
            |o, v| o.mcr_dynamic_lights = v,
        ));
    }
    rows.extend(small(preferences));
    OptionsPage::new("options.videoTitle", host.settings.clone(), rows)
}

pub fn controls(host: &Host) -> OptionsPage {
    let (mouse_host, keys_host) = (host.clone(), host.clone());
    let mut rows = vec![Row::Small(
        Opt::button("options.mouse_settings", move |_| push(mouse(&mouse_host))),
        Some(Opt::button("controls.keybinds", move |_| {
            push(crate::key_binds::KeyBindsScreen::new(
                keys_host.settings.clone(),
            ))
        })),
    )];
    rows.extend(small(vec![
        toggle_key("key.sneak", |o| o.toggle_crouch, |o, v| o.toggle_crouch = v),
        toggle_key(
            "key.sprint",
            |o| o.toggle_sprint,
            |o, v| o.toggle_sprint = v,
        ),
        toggle_key(
            "key.attack",
            |o| o.toggle_attack,
            |o, v| o.toggle_attack = v,
        ),
        toggle_key("key.use", |o| o.toggle_use, |o, v| o.toggle_use = v),
        Opt::toggle("options.autoJump", |o| o.auto_jump, |o, v| o.auto_jump = v),
        Opt::int(
            (0, 10),
            |o| o.sprint_window,
            |o, v| o.sprint_window = v,
            |cx, v| {
                if v == 0 {
                    generic_key(cx, "options.sprintWindow", "options.off")
                } else {
                    let n = cx.lang_fmt("options.value", &[&v.to_string()]);
                    generic(cx, "options.sprintWindow", &n)
                }
            },
        )
        .tooltip("options.sprintWindow.tooltip"),
        Opt::toggle(
            "options.quitShortcuts",
            |o| o.quit_shortcuts,
            |o, v| o.quit_shortcuts = v,
        )
        .tooltip("options.quitShortcuts.tooltip"),
        Opt::toggle(
            "options.operatorItemsTab",
            |o| o.operator_items_tab,
            |o, v| o.operator_items_tab = v,
        ),
    ]));
    OptionsPage::new("controls.title", host.settings.clone(), rows)
}

pub fn mouse(host: &Host) -> OptionsPage {
    let rows = small(vec![
        Opt::unit(
            |o| o.mouse_sensitivity,
            |o, v| o.mouse_sensitivity = v,
            |cx, v| {
                if v == 0.0 {
                    generic_key(cx, "options.sensitivity", "options.sensitivity.min")
                } else if v == 1.0 {
                    generic_key(cx, "options.sensitivity", "options.sensitivity.max")
                } else {
                    percent(cx, "options.sensitivity", 2.0 * v)
                }
            },
        ),
        Opt::int(
            (-200, 100),
            |o| (o.mouse_wheel_sensitivity.max(0.01).log10() * 100.0).floor() as i32,
            |o, v| o.mouse_wheel_sensitivity = 10f64.powf(f64::from(v) / 100.0),
            |cx, v| {
                let value = format!("{:.2}", 10f64.powf(f64::from(v) / 100.0));
                generic(cx, "options.mouseWheelSensitivity", &value)
            },
        ),
        Opt::toggle(
            "options.discrete_mouse_scroll",
            |o| o.discrete_mouse_scroll,
            |o, v| o.discrete_mouse_scroll = v,
        ),
        Opt::toggle(
            "options.invertMouseX",
            |o| o.invert_x_mouse,
            |o, v| o.invert_x_mouse = v,
        ),
        Opt::toggle(
            "options.invertMouseY",
            |o| o.invert_y_mouse,
            |o, v| o.invert_y_mouse = v,
        ),
        Opt::toggle(
            "options.allowCursorChanges",
            |o| o.allow_cursor_changes,
            |o, v| o.allow_cursor_changes = v,
        )
        .tooltip("options.allowCursorChanges.tooltip"),
    ]);
    OptionsPage::new("options.mouse_settings.title", host.settings.clone(), rows)
}

pub fn accessibility(host: &Host) -> OptionsPage {
    let controls_host = host.clone();
    let has_high_contrast = host.packs.has_builtin(crate::packs::HIGH_CONTRAST);
    let minecart = host.minecart_improvements;
    let mut rows = vec![Row::Small(
        narrator(),
        Some(Opt::button("options.controls", move |_| {
            push(controls(&controls_host))
        })),
    )];
    let mut high_contrast = Opt::toggle(
        "options.accessibility.high_contrast",
        |o| o.high_contrast,
        |o, v| {
            o.high_contrast = v;
            crate::packs::select_builtin(o, crate::packs::HIGH_CONTRAST, v);
        },
    );
    high_contrast = if has_high_contrast {
        high_contrast.tooltip("options.accessibility.high_contrast.tooltip")
    } else {
        high_contrast
            .inactive()
            .tooltip("options.accessibility.high_contrast.error.tooltip")
    };
    rows.extend(small(vec![
        show_subtitles(),
        high_contrast,
        Opt::int(
            (0, 10),
            |o| o.menu_background_blurriness,
            |o, v| o.menu_background_blurriness = v,
            |cx, v| {
                let caption = "options.accessibility.menu_background_blurriness";
                if v == 0 {
                    generic_key(cx, caption, "options.off")
                } else {
                    generic(cx, caption, &v.to_string())
                }
            },
        )
        .tooltip("options.accessibility.menu_background_blurriness.tooltip"),
        text_background_opacity(),
        Opt::toggle_labels(
            "options.accessibility.text_background",
            |o| o.background_for_chat_only,
            |o, v| o.background_for_chat_only = v,
            "options.accessibility.text_background.chat",
            "options.accessibility.text_background.everywhere",
        ),
        chat_opacity(),
        line_spacing(),
        chat_delay_slider(),
        Opt::int(
            (5, 100),
            |o| (o.notification_display_time * 10.0).round() as i32,
            |o, v| o.notification_display_time = f64::from(v) / 10.0,
            |cx, v| {
                let factor = cx.lang_fmt(
                    "options.multiplier",
                    &[&format!("{:?}", f64::from(v) / 10.0)],
                );
                generic(cx, "options.notifications.display_time", &factor)
            },
        )
        .tooltip("options.notifications.display_time.tooltip"),
        Opt::toggle("options.viewBobbing", |o| o.bob_view, |o, v| o.bob_view = v),
        scale(
            "options.screenEffectScale",
            "options.screenEffectScale.tooltip",
            |o| o.screen_effect_scale,
            |o, v| o.screen_effect_scale = v,
        ),
        squared(
            "options.fovEffectScale",
            "options.fovEffectScale.tooltip",
            |o| o.fov_effect_scale,
            |o, v| o.fov_effect_scale = v,
        ),
        squared(
            "options.darknessEffectScale",
            "options.darknessEffectScale.tooltip",
            |o| o.darkness_effect_scale,
            |o, v| o.darkness_effect_scale = v,
        ),
        scale(
            "options.damageTiltStrength",
            "options.damageTiltStrength.tooltip",
            |o| o.damage_tilt_strength,
            |o, v| o.damage_tilt_strength = v,
        ),
        scale(
            "options.glintSpeed",
            "options.glintSpeed.tooltip",
            |o| o.glint_speed,
            |o, v| o.glint_speed = v,
        ),
        scale(
            "options.glintStrength",
            "options.glintStrength.tooltip",
            |o| o.glint_strength,
            |o, v| o.glint_strength = v,
        ),
        Opt::toggle(
            "options.hideLightningFlashes",
            |o| o.hide_lightning_flashes,
            |o, v| o.hide_lightning_flashes = v,
        )
        .tooltip("options.hideLightningFlashes.tooltip"),
        Opt::toggle(
            "options.darkMojangStudiosBackgroundColor",
            |o| o.dark_mojang_studios_background,
            |o, v| o.dark_mojang_studios_background = v,
        )
        .tooltip("options.darkMojangStudiosBackgroundColor.tooltip"),
        opacity(
            "options.accessibility.panorama_speed",
            |o| o.panorama_scroll_speed,
            |o, v| o.panorama_scroll_speed = v,
        ),
        Opt::toggle(
            "options.hideSplashTexts",
            |o| o.hide_splash_texts,
            |o, v| o.hide_splash_texts = v,
        )
        .tooltip("options.hideSplashTexts.tooltip"),
        Opt::toggle(
            "options.accessibility.narrator_hotkey",
            |o| o.narrator_hotkey,
            |o, v| o.narrator_hotkey = v,
        )
        .tooltip("options.accessibility.narrator_hotkey.tooltip"),
        Opt::toggle(
            "options.rotateWithMinecart",
            |o| o.rotate_with_minecart,
            |o, v| o.rotate_with_minecart = v,
        )
        .active_if(move |_| minecart)
        .tooltip("options.rotateWithMinecart.tooltip"),
        Opt::toggle(
            "options.accessibility.high_contrast_block_outline",
            |o| o.high_contrast_block_outline,
            |o, v| o.high_contrast_block_outline = v,
        )
        .tooltip("options.accessibility.high_contrast_block_outline.tooltip"),
    ]));
    OptionsPage::new("options.accessibility.title", host.settings.clone(), rows).with_footer(vec![
        FooterButton::Custom(
            "options.accessibility.link",
            Rc::new(|_| link(ACCESSIBILITY_HELP)),
        ),
        FooterButton::Done,
    ])
}

pub fn online(host: &Host) -> OptionsPage {
    let off = |caption: &'static str| {
        Opt::cycle_with(
            1,
            |_| 0,
            |_, _| {},
            move |_, cx| generic_key(cx, caption, "options.off"),
        )
        .inactive()
    };
    let rows = vec![
        Row::Header("options.online.friends.header"),
        Row::Small(
            off("options.friendsList"),
            Some(off("options.allowFriendRequests").tooltip("options.allowFriendRequests.tooltip")),
        ),
        Row::Small(
            Opt::toggle(
                "options.inGameNotification",
                |o| o.in_game_notification,
                |o, v| o.in_game_notification = v,
            )
            .inactive()
            .tooltip("options.inGameNotification.tooltip"),
            Some(
                Opt::cycle(
                    "options.sharePresence",
                    PresenceSharing::ALL,
                    |o| o.share_presence,
                    |o, v| o.share_presence = v,
                    presence_key,
                )
                .inactive()
                .tooltip_with(|o, cx| {
                    cx.lang(&format!("{}.tooltip", presence_key(o.share_presence)))
                }),
            ),
        ),
        Row::Big(Opt::button("options.online.xboxSettings", |_| {
            link(PRIVACY_AND_ONLINE_SETTINGS)
        })),
        Row::Header("options.online.servers.header"),
        Row::Big(
            Opt::toggle(
                "options.allowServerListing",
                |o| o.allow_server_listing,
                |o, v| o.allow_server_listing = v,
            )
            .tooltip("options.allowServerListing.tooltip"),
        ),
        Row::Header("options.online.realms.header"),
        Row::Big(
            Opt::toggle(
                "options.realmsNotifications",
                |o| o.realms_notifications,
                |o, v| o.realms_notifications = v,
            )
            .tooltip("options.realmsNotifications.tooltip"),
        ),
    ];
    OptionsPage::new("options.online.title", host.settings.clone(), rows)
}

fn presence_key(v: PresenceSharing) -> &'static str {
    match v {
        PresenceSharing::None => "options.sharePresence.none",
        PresenceSharing::Limited => "options.sharePresence.limited",
        PresenceSharing::All => "options.sharePresence.all",
    }
}

pub fn chat(host: &Host) -> OptionsPage {
    let width = |v: f64| (v * 280.0 + 40.0).floor() as i32;
    let height = |v: f64| (v * 160.0 + 20.0).floor() as i32;
    let rows = small(vec![
        Opt::cycle(
            "options.chat.visibility",
            ChatVisibility::ALL,
            |o| o.chat_visibility,
            |o, v| o.chat_visibility = v,
            |v| match v {
                ChatVisibility::Full => "options.chat.visibility.full",
                ChatVisibility::System => "options.chat.visibility.system",
                ChatVisibility::Hidden => "options.chat.visibility.hidden",
            },
        ),
        Opt::toggle(
            "options.chat.color",
            |o| o.chat_colors,
            |o, v| o.chat_colors = v,
        ),
        Opt::toggle(
            "options.chat.links",
            |o| o.chat_links,
            |o, v| o.chat_links = v,
        ),
        Opt::toggle(
            "options.chat.links.prompt",
            |o| o.chat_links_prompt,
            |o, v| o.chat_links_prompt = v,
        ),
        chat_opacity(),
        text_background_opacity(),
        Opt::unit(
            |o| o.chat_scale,
            |o, v| o.chat_scale = v,
            |cx, v| {
                if v == 0.0 {
                    status(cx, "options.chat.scale", false)
                } else {
                    percent(cx, "options.chat.scale", v)
                }
            },
        ),
        line_spacing(),
        chat_delay_slider(),
        Opt::unit(
            |o| o.chat_width,
            |o, v| o.chat_width = v,
            move |cx, v| pixels(cx, "options.chat.width", width(v)),
        ),
        Opt::unit(
            |o| o.chat_height_focused,
            |o, v| o.chat_height_focused = v,
            move |cx, v| pixels(cx, "options.chat.height.focused", height(v)),
        ),
        Opt::unit(
            |o| o.chat_height_unfocused,
            |o, v| o.chat_height_unfocused = v,
            move |cx, v| pixels(cx, "options.chat.height.unfocused", height(v)),
        ),
        narrator(),
        Opt::toggle(
            "options.autoSuggestCommands",
            |o| o.auto_suggestions,
            |o, v| o.auto_suggestions = v,
        ),
        Opt::toggle(
            "options.hideMatchedNames",
            |o| o.hide_matched_names,
            |o, v| o.hide_matched_names = v,
        )
        .tooltip("options.hideMatchedNames.tooltip"),
        Opt::toggle(
            "options.reducedDebugInfo",
            |o| o.reduced_debug_info,
            |o, v| o.reduced_debug_info = v,
        ),
        Opt::toggle(
            "options.onlyShowSecureChat",
            |o| o.only_show_secure_chat,
            |o, v| o.only_show_secure_chat = v,
        )
        .tooltip("options.onlyShowSecureChat.tooltip"),
        Opt::toggle(
            "options.chat.drafts",
            |o| o.save_chat_drafts,
            |o, v| o.save_chat_drafts = v,
        )
        .tooltip("options.chat.drafts.tooltip"),
    ]);
    OptionsPage::new("options.chat.title", host.settings.clone(), rows)
}

pub fn font(host: &Host) -> OptionsPage {
    let rows = small(vec![
        Opt::toggle(
            "options.forceUnicodeFont",
            |o| o.force_unicode_font,
            |o, v| o.force_unicode_font = v,
        ),
        Opt::toggle(
            "options.japaneseGlyphVariants",
            |o| o.japanese_glyph_variants,
            |o, v| o.japanese_glyph_variants = v,
        )
        .tooltip("options.japaneseGlyphVariants.tooltip"),
    ]);
    OptionsPage::new("options.font.title", host.settings.clone(), rows)
}

const FOV: u32 = 1;
const ONLINE: u32 = 2;
const DONE: u32 = 3;
const GRID: [(u32, &str); 10] = [
    (10, "options.skinCustomisation"),
    (11, "options.sounds"),
    (12, "options.video"),
    (13, "options.controls"),
    (14, "options.language"),
    (15, "options.chat"),
    (16, "options.resourcepack"),
    (17, "options.accessibility"),
    (18, "options.telemetry"),
    (19, "options.credits_and_attribution"),
];
const TELEMETRY: u32 = 18;
const HEADER_BIG: i32 = 61;

/// `OptionsScreen`: FOV and Online on top, then a grid of the option pages.
pub struct OptionsScreen {
    host: Host,
    widgets: WidgetSet,
    gui: (i32, i32),
}

impl std::fmt::Debug for OptionsScreen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OptionsScreen").finish_non_exhaustive()
    }
}

impl OptionsScreen {
    pub fn new(host: OptionsHost) -> Self {
        Self {
            host: Rc::new(host),
            widgets: WidgetSet::default(),
            gui: (0, 0),
        }
    }

    fn fov(&self) -> i32 {
        self.host.settings.borrow().file.options.fov.0
    }

    fn open(&self, id: u32) -> Vec<ScreenAction> {
        let host = &self.host;
        match id {
            ONLINE => push(online(host)),
            10 => push(skin(host)),
            11 => push(sound(host)),
            12 => push(video(host)),
            13 => push(controls(host)),
            14 => {
                let font_host = host.clone();
                push(
                    LanguageScreen::new(host.settings.clone(), host.languages.clone())
                        .with_font_settings(move || Box::new(font(&font_host))),
                )
            }
            15 => push(chat(host)),
            16 => push(crate::packs::PackScreen::new(
                host.settings.clone(),
                host.packs.clone(),
            )),
            17 => push(accessibility(host)),
            19 => push(CreditsAndAttribution::new(host.clone())),
            _ => Vec::new(),
        }
    }
}

impl Screen for OptionsScreen {
    fn init(&mut self, width: i32, height: i32) {
        self.gui = (width, height);
        self.widgets.clear();
        let (left, right) = (width / 2 - 154, width / 2 + 4);
        let fov = self.fov();
        self.widgets.add(Slider::new(
            FOV,
            [left, 29, 150, 20],
            "",
            (fov - 30) as f32 / 80.0,
        ));
        self.widgets.add(Button::new(
            ONLINE,
            [right, 29, 150, 20],
            "lang:options.online",
        ));
        let rows = GRID.len().div_ceil(2) as i32;
        let top = HEADER_BIG + ((height - HEADER_BIG - FOOTER) - rows * 24).max(0) / 2;
        for (i, (id, key)) in GRID.iter().enumerate() {
            let x = if i % 2 == 0 { left } else { right };
            let y = top + (i as i32 / 2) * 24;
            let mut b = Button::new(*id, [x, y, 150, 20], format!("lang:{key}"));
            if *id == TELEMETRY {
                b.active = false;
                let mut tip = TooltipLines::default();
                tip.push_component(mcr_protocol::types::text::Component::translatable(
                    "options.telemetry.disabled",
                    Vec::new(),
                ));
                b.tooltip = Some(tip);
            }
            self.widgets.add(b);
        }
        self.widgets.add(Button::new(
            DONE,
            [width / 2 - 100, height - FOOTER + 6, 200, 20],
            "lang:gui.done",
        ));
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        draw_menu_background(cx);
        let title = cx.lang("options.title");
        cx.centered_text(&title, self.gui.0 / 2, 12, WHITE, true);
        let label = fov_label(cx, self.fov());
        if let Some(s) = self.widgets.slider_mut(FOV) {
            s.label = label;
        }
        self.widgets.render(cx);
    }

    fn input(&mut self, env: &mut Env<'_>, input: &UiInput, out: &mut Vec<ScreenAction>) -> bool {
        if let UiInput::Key {
            key: Key::Escape, ..
        } = input
        {
            out.push(ScreenAction::Pop);
            return true;
        }
        let mut events = Vec::new();
        let used = self.widgets.input(env, input, &mut events);
        for e in events {
            match (e.id, e.kind) {
                (FOV, WidgetEventKind::SliderChanged(v)) => {
                    let fov = 30 + (v * 80.0).round() as i32;
                    self.host
                        .settings
                        .borrow_mut()
                        .edit(|f| f.options.fov = Fov(fov));
                    if let Some(s) = self.widgets.slider_mut(FOV) {
                        s.set_value((fov - 30) as f32 / 80.0);
                    }
                }
                (DONE, WidgetEventKind::Pressed) => {
                    out.push(ScreenAction::ClickSound);
                    out.push(ScreenAction::Pop);
                }
                (id, WidgetEventKind::Pressed) => {
                    out.push(ScreenAction::ClickSound);
                    out.extend(self.open(id));
                }
                _ => {}
            }
        }
        used || matches!(input, UiInput::Key { .. } | UiInput::Char(_))
    }
}

const CREDITS: u32 = 1;
const ATTRIBUTION_ID: u32 = 2;
const LICENSES_ID: u32 = 3;
const CREDITS_DONE: u32 = 4;

/// `CreditsAndAttributionScreen`.
pub struct CreditsAndAttribution {
    host: Host,
    widgets: WidgetSet,
    gui: (i32, i32),
}

impl std::fmt::Debug for CreditsAndAttribution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CreditsAndAttribution")
            .finish_non_exhaustive()
    }
}

impl CreditsAndAttribution {
    fn new(host: Host) -> Self {
        Self {
            host,
            widgets: WidgetSet::default(),
            gui: (0, 0),
        }
    }
}

impl Screen for CreditsAndAttribution {
    fn init(&mut self, width: i32, height: i32) {
        self.gui = (width, height);
        self.widgets.clear();
        let header = crate::options_list::HEADER;
        let top = header + ((height - header - FOOTER) - (3 * 20 + 2 * 8)).max(0) / 2;
        for (i, (id, key)) in [
            (CREDITS, "credits_and_attribution.button.credits"),
            (ATTRIBUTION_ID, "credits_and_attribution.button.attribution"),
            (LICENSES_ID, "credits_and_attribution.button.licenses"),
        ]
        .into_iter()
        .enumerate()
        {
            self.widgets.add(Button::new(
                id,
                [width / 2 - 105, top + i as i32 * 28, 210, 20],
                format!("lang:{key}"),
            ));
        }
        self.widgets.add(Button::new(
            CREDITS_DONE,
            [width / 2 - 100, height - FOOTER + 6, 200, 20],
            "lang:gui.done",
        ));
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        draw_menu_background(cx);
        let title = cx.lang("credits_and_attribution.screen.title");
        cx.centered_text(&title, self.gui.0 / 2, 12, WHITE, true);
        self.widgets.render(cx);
    }

    fn input(&mut self, env: &mut Env<'_>, input: &UiInput, out: &mut Vec<ScreenAction>) -> bool {
        if let UiInput::Key {
            key: Key::Escape, ..
        } = input
        {
            out.push(ScreenAction::Pop);
            return true;
        }
        let mut events = Vec::new();
        let used = self.widgets.input(env, input, &mut events);
        for e in events.iter().filter(|e| e.kind == WidgetEventKind::Pressed) {
            out.push(ScreenAction::ClickSound);
            match e.id {
                CREDITS => {
                    let source = (self.host.credits)();
                    out.extend(push(CreditsScreen::new(&source, false, &self.host.player)));
                }
                ATTRIBUTION_ID => out.extend(link(ATTRIBUTION)),
                LICENSES_ID => out.extend(link(LICENSES)),
                CREDITS_DONE => out.push(ScreenAction::Pop),
                _ => {}
            }
        }
        used || matches!(input, UiInput::Key { .. } | UiInput::Char(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(name: &str) -> Host {
        let dir = std::env::temp_dir().join(format!("mcr-pages-{name}-{}", std::process::id()));
        Rc::new(OptionsHost {
            settings: crate::settings::Settings::load(&dir),
            languages: Vec::new(),
            max_gui_scale: 4,
            credits: Rc::new(CreditsSource::default),
            player: "Steve".into(),
            minecart_improvements: false,
            packs: crate::packs::PackHost::default(),
        })
    }

    #[test]
    fn video_lists_vanilla_options_and_the_mod_light_switch_only_with_a_provider() {
        let h = host("video");
        let plain = video(&h).option_count();
        assert_eq!(plain, 1 + 8 + 1 + 17 + 4);
        h.settings.borrow_mut().dynamic_lights_provider = true;
        assert_eq!(video(&h).option_count(), plain + 1);
    }

    #[test]
    fn skin_page_sets_parts_and_main_hand() {
        let h = host("skin");
        let mut page = skin(&h);
        assert_eq!(page.option_count(), MODEL_PARTS.len() + 1);
        page.click(0);
        page.click(MODEL_PARTS.len());
        let o = h.settings.borrow().file.options.clone();
        assert!(!o.model_parts[0]);
        assert_eq!(o.main_hand, HumanoidArm::Left);
        let info = crate::settings::client_information(&o, 8);
        assert_eq!(info.model_customisation, 0x7e);
    }

    #[test]
    fn quality_changes_make_the_preset_custom() {
        let h = host("preset");
        h.settings
            .borrow_mut()
            .edit(|f| f.options.apply_graphics_preset(GraphicsPreset::Fancy));
        let mut page = video(&h);
        let ao = 1 + 8 + 1 + 4;
        page.click(ao);
        let o = h.settings.borrow().file.options.clone();
        assert!(!o.ambient_occlusion);
        assert_eq!(o.graphics_preset, GraphicsPreset::Custom);
    }
}
