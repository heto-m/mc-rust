use crate::value::{
    AttackIndicator, ChatVisibility, CloudStatus, Fov, GraphicsPreset, HumanoidArm,
    InactivityFpsLimit, MusicFrequency, MusicToastState, NarratorStatus, OptionValue,
    ParticleStatus, PreferredGraphicsApi, PresenceSharing, PrioritizeChunkUpdates, QuotedString,
    StringList, TextureFiltering,
};

pub const SOUND_CATEGORIES: [&str; 11] = [
    "master", "music", "record", "weather", "block", "hostile", "neutral", "player", "ambient",
    "voice", "ui",
];

pub const MODEL_PARTS: [&str; 7] = [
    "cape",
    "jacket",
    "left_sleeve",
    "right_sleeve",
    "left_pants_leg",
    "right_pants_leg",
    "hat",
];

macro_rules! options {
    ($( $field:ident : $ty:ty = $key:literal => $default:expr $(, [$lo:expr, $hi:expr])? ; )*) => {
        /// Every `options.txt` client option of Minecraft 26.3 except sound volumes, model parts and key bindings.
        #[derive(Debug, Clone, PartialEq)]
        pub struct Options {
            $(pub $field: $ty,)*
            /// Indexed like [`SOUND_CATEGORIES`].
            pub sound_volumes: [f64; SOUND_CATEGORIES.len()],
            /// Indexed like [`MODEL_PARTS`].
            pub model_parts: [bool; MODEL_PARTS.len()],
        }

        impl Default for Options {
            fn default() -> Self {
                Self {
                    $($field: $default,)*
                    sound_volumes: [1.0; SOUND_CATEGORIES.len()],
                    model_parts: [true; MODEL_PARTS.len()],
                }
            }
        }

        impl Options {
            pub(crate) fn entries(&self) -> Vec<(&'static str, String)> {
                let mut out: Vec<(&'static str, String)> = Vec::new();
                $(out.push(($key, self.$field.format()));)*
                out
            }

            pub(crate) fn apply(&mut self, key: &str, raw: &str) -> bool {
                match key {
                    $($key => {
                        let bounds: &[(f64, f64)] = &[$(($lo as f64, $hi as f64))?];
                        match <$ty as OptionValue>::parse(raw) {
                            Some(v) => self.$field = v.clamp_to(bounds.first().copied()),
                            None => {
                                tracing::warn!(key, raw, "invalid option value, using default");
                                self.$field = $default;
                            }
                        }
                    })*
                    _ => return false,
                }
                true
            }
        }
    };
}

options! {
    ambient_occlusion: bool = "ao" => true;
    biome_blend_radius: i32 = "biomeBlendRadius" => 2, [0, 7];
    chunk_section_fade_in_time: f64 = "chunkSectionFadeInTime" => 0.75, [0.0, 2.0];
    cutout_leaves: bool = "cutoutLeaves" => true;
    enable_vsync: bool = "enableVsync" => true;
    entity_distance_scaling: f64 = "entityDistanceScaling" => 1.0, [0.5, 5.0];
    entity_shadows: bool = "entityShadows" => true;
    force_unicode_font: bool = "forceUnicodeFont" => false;
    japanese_glyph_variants: bool = "japaneseGlyphVariants" => false;
    fov: Fov = "fov" => Fov(70), [30, 110];
    fov_effect_scale: f64 = "fovEffectScale" => 1.0, [0.0, 1.0];
    darkness_effect_scale: f64 = "darknessEffectScale" => 1.0, [0.0, 1.0];
    glint_speed: f64 = "glintSpeed" => 0.5, [0.0, 1.0];
    glint_strength: f64 = "glintStrength" => 0.75, [0.0, 1.0];
    preferred_graphics_backend: PreferredGraphicsApi = "preferredGraphicsBackend" => PreferredGraphicsApi::Default;
    graphics_preset: GraphicsPreset = "graphicsPreset" => GraphicsPreset::Fancy;
    prioritize_chunk_updates: PrioritizeChunkUpdates = "prioritizeChunkUpdates" => PrioritizeChunkUpdates::None;
    fullscreen: bool = "fullscreen" => false;
    exclusive_fullscreen: bool = "exclusiveFullscreen" => false;
    mac_fullscreen_menu_visibility: bool = "macFullscreenMenuVisibility" => false;
    gamma: f64 = "gamma" => 0.5, [0.0, 1.0];
    gui_scale: i32 = "guiScale" => 0, [0, 2147483646];
    max_anisotropy_bit: i32 = "maxAnisotropyBit" => 2, [1, 3];
    texture_filtering: TextureFiltering = "textureFiltering" => TextureFiltering::None;
    max_fps: i32 = "maxFps" => 120, [10, 260];
    improved_transparency: bool = "improvedTransparency" => false;
    inactivity_fps_limit: InactivityFpsLimit = "inactivityFpsLimit" => InactivityFpsLimit::Afk;
    mipmap_levels: i32 = "mipmapLevels" => 4, [0, 4];
    narrator: NarratorStatus = "narrator" => NarratorStatus::Off;
    particles: ParticleStatus = "particles" => ParticleStatus::All;
    reduced_debug_info: bool = "reducedDebugInfo" => false;
    render_clouds: CloudStatus = "renderClouds" => CloudStatus::Fancy;
    cloud_range: i32 = "cloudRange" => 128, [2, 128];
    render_distance: i32 = "renderDistance" => 32, [2, 32];
    simulation_distance: i32 = "simulationDistance" => 12, [5, 32];
    screen_effect_scale: f64 = "screenEffectScale" => 1.0, [0.0, 1.0];
    sound_device: QuotedString = "soundDevice" => QuotedString::default();
    vignette: bool = "vignette" => true;
    weather_radius: i32 = "weatherRadius" => 10, [3, 10];
    auto_jump: bool = "autoJump" => false;
    rotate_with_minecart: bool = "rotateWithMinecart" => false;
    operator_items_tab: bool = "operatorItemsTab" => false;
    auto_suggestions: bool = "autoSuggestions" => true;
    chat_colors: bool = "chatColors" => true;
    chat_links: bool = "chatLinks" => true;
    chat_links_prompt: bool = "chatLinksPrompt" => true;
    discrete_mouse_scroll: bool = "discrete_mouse_scroll" => false;
    invert_x_mouse: bool = "invertXMouse" => false;
    invert_y_mouse: bool = "invertYMouse" => false;
    realms_notifications: bool = "realmsNotifications" => true;
    show_subtitles: bool = "showSubtitles" => false;
    directional_audio: bool = "directionalAudio" => false;
    bob_view: bool = "bobView" => true;
    toggle_crouch: bool = "toggleCrouch" => false;
    toggle_sprint: bool = "toggleSprint" => false;
    toggle_attack: bool = "toggleAttack" => false;
    toggle_use: bool = "toggleUse" => false;
    sprint_window: i32 = "sprintWindow" => 7, [0, 10];
    dark_mojang_studios_background: bool = "darkMojangStudiosBackground" => false;
    hide_lightning_flashes: bool = "hideLightningFlashes" => false;
    hide_splash_texts: bool = "hideSplashTexts" => false;
    mouse_sensitivity: f64 = "mouseSensitivity" => 0.5, [0.0, 1.0];
    damage_tilt_strength: f64 = "damageTiltStrength" => 1.0, [0.0, 1.0];
    high_contrast: bool = "highContrast" => false;
    high_contrast_block_outline: bool = "highContrastBlockOutline" => false;
    narrator_hotkey: bool = "narratorHotkey" => true;
    resource_packs: StringList = "resourcePacks" => StringList::default();
    incompatible_resource_packs: StringList = "incompatibleResourcePacks" => StringList::default();
    last_server: String = "lastServer" => String::new();
    language: String = "lang" => "en_us".to_owned();
    chat_visibility: ChatVisibility = "chatVisibility" => ChatVisibility::Full;
    chat_opacity: f64 = "chatOpacity" => 1.0, [0.0, 1.0];
    chat_line_spacing: f64 = "chatLineSpacing" => 0.0, [0.0, 1.0];
    text_background_opacity: f64 = "textBackgroundOpacity" => 0.5, [0.0, 1.0];
    background_for_chat_only: bool = "backgroundForChatOnly" => true;
    hide_server_address: bool = "hideServerAddress" => false;
    advanced_item_tooltips: bool = "advancedItemTooltips" => false;
    pause_on_lost_focus: bool = "pauseOnLostFocus" => true;
    override_width: i32 = "overrideWidth" => 0;
    override_height: i32 = "overrideHeight" => 0;
    chat_height_focused: f64 = "chatHeightFocused" => 1.0, [0.0, 1.0];
    chat_delay: f64 = "chatDelay" => 0.0, [0.0, 6.0];
    chat_height_unfocused: f64 = "chatHeightUnfocused" => 0.4375, [0.0, 1.0];
    chat_scale: f64 = "chatScale" => 1.0, [0.0, 1.0];
    chat_width: f64 = "chatWidth" => 1.0, [0.0, 1.0];
    notification_display_time: f64 = "notificationDisplayTime" => 1.0, [0.5, 10.0];
    use_native_transport: bool = "useNativeTransport" => true;
    main_hand: HumanoidArm = "mainHand" => HumanoidArm::Right;
    attack_indicator: AttackIndicator = "attackIndicator" => AttackIndicator::Crosshair;
    tutorial_step: String = "tutorialStep" => "movement".to_owned();
    mouse_wheel_sensitivity: f64 = "mouseWheelSensitivity" => 1.0, [0.01, 10.0];
    allow_cursor_changes: bool = "allowCursorChanges" => true;
    quit_shortcuts: bool = "quitShortcuts" => true;
    ctrl_click_emulates_right_click: bool = "ctrlClickEmulatesRightClick" => false;
    skip_multiplayer_warning: bool = "skipMultiplayerWarning" => false;
    hide_matched_names: bool = "hideMatchedNames" => true;
    joined_first_server: bool = "joinedFirstServer" => false;
    sync_chunk_writes: bool = "syncChunkWrites" => false;
    show_autosave_indicator: bool = "showAutosaveIndicator" => true;
    allow_server_listing: bool = "allowServerListing" => true;
    in_game_notification: bool = "inGameNotification" => false;
    share_presence: PresenceSharing = "sharePresence" => PresenceSharing::All;
    only_show_secure_chat: bool = "onlyShowSecureChat" => false;
    save_chat_drafts: bool = "saveChatDrafts" => false;
    panorama_scroll_speed: f64 = "panoramaScrollSpeed" => 1.0, [0.0, 1.0];
    telemetry_opt_in_extra: bool = "telemetryOptInExtra" => false;
    onboard_accessibility: bool = "onboardAccessibility" => true;
    menu_background_blurriness: i32 = "menuBackgroundBlurriness" => 5, [0, 10];
    started_cleanly: bool = "startedCleanly" => true;
    music_toast: MusicToastState = "musicToast" => MusicToastState::Never;
    music_frequency: MusicFrequency = "musicFrequency" => MusicFrequency::Default;
}

impl Options {
    pub fn sound_volume(&self, category: &str) -> Option<f64> {
        SOUND_CATEGORIES
            .iter()
            .position(|c| *c == category)
            .map(|i| self.sound_volumes[i])
    }

    pub(crate) fn apply_sound_or_part(&mut self, key: &str, raw: &str) -> bool {
        if let Some(name) = key.strip_prefix("soundCategory_")
            && let Some(i) = SOUND_CATEGORIES.iter().position(|c| *c == name)
        {
            match f64::parse(raw) {
                Some(v) => self.sound_volumes[i] = v.clamp(0.0, 1.0),
                None => {
                    tracing::warn!(key, raw, "invalid option value, using default");
                    self.sound_volumes[i] = 1.0;
                }
            }
            return true;
        }
        if let Some(name) = key.strip_prefix("modelPart_")
            && let Some(i) = MODEL_PARTS.iter().position(|p| *p == name)
        {
            match bool::parse(raw) {
                Some(v) => self.model_parts[i] = v,
                None => {
                    tracing::warn!(key, raw, "invalid option value, using default");
                    self.model_parts[i] = true;
                }
            }
            return true;
        }
        false
    }
}
