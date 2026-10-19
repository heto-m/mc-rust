mod assets;
mod atlas;
mod error;
mod font;
mod gui;
mod items;
mod lang;
mod renderer;
mod screens;
mod sprites;
mod style;
mod text;
mod toast;
mod ui;

pub use assets::{AssetIndexSource, vanilla_resources};
pub use atlas::{Atlas, Slot};
pub use error::{Result, UiError};
pub use font::{Baked, FontOptions, Fonts, GlyphInfo};
pub use gui::*;
pub use items::{
    BundleWeight, EffectCategory, EffectInfo, IconStats, ItemDefaults, ItemIcons, ItemRef,
    ItemView, MAP_SIZE, MapInfo, NoTooltipSource, RegistryRef, StackView, TooltipFlags,
    TooltipSource, bundle_bar, bundle_contents, bundle_items, bundle_weight, durability_bar,
    effect_description, effect_info, enchantment_name, format_amount, format_ticks, hover_name,
    is_bundle, item_defaults, item_weight, items_to_show, map_color, map_rgba, rarity_color,
    styled_hover_name, tooltip_lines, weight_safe,
};
pub use lang::{DEFAULT_LANGUAGE, LanguageInfo, available_languages};
pub use renderer::{UiRenderer, UiStats};
pub use screens::{
    AirBubbles, AirState, CHART_SAMPLES, ChatHud, ChatLineTag, ChatOptions, ContextualBar,
    CrosshairIndicator, DebugChart, DebugOverlay, HeartKind, HudEffect, HudItems, HudSprites,
    HudState, MAX_VEHICLE_HEARTS, effect_alpha, fade_alpha,
};
pub use sprites::{
    OwnedSprites, Scaling, SpriteId, SpriteInfo, SpriteQuad, SpriteReleaser, Sprites,
};
pub use style::{Cell, Flags, FontId, Style};
pub use text::{
    ClickSpan, Lang, click_at, flatten, flatten_events, parse_legacy, push_legacy, wrap,
};
pub use toast::{SystemToastId, Toast, ToastFrame, ToastManager, note_color};
pub use ui::{
    Batch, Blend, DrawCmd, LINE_HEIGHT, PageKind, PageRef, Ui, UiFrame, Vertex, gui_scale,
};

pub use wgpu;
