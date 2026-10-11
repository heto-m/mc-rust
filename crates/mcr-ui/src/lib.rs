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
pub use items::{IconStats, ItemIcons, ItemView, StackView, durability_bar};
pub use lang::{DEFAULT_LANGUAGE, LanguageInfo, available_languages};
pub use renderer::{UiRenderer, UiStats};
pub use screens::{
    ChatHud, ChatOptions, DebugOverlay, HeartKind, HudItems, HudSprites, HudState, fade_alpha,
};
pub use sprites::{Scaling, SpriteId, SpriteInfo, SpriteQuad, Sprites};
pub use style::{Cell, Flags, FontId, Style};
pub use text::{
    ClickSpan, Lang, click_at, flatten, flatten_events, parse_legacy, push_legacy, wrap,
};
pub use toast::{SystemToastId, Toast, ToastFrame, ToastManager, note_color};
pub use ui::{Batch, DrawCmd, LINE_HEIGHT, PageKind, PageRef, Ui, UiFrame, Vertex, gui_scale};

pub use wgpu;
