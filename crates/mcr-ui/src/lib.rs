mod assets;
mod atlas;
mod error;
mod font;
mod renderer;
mod screens;
mod sprites;
mod style;
mod text;
mod ui;

pub use assets::{AssetIndexSource, vanilla_resources};
pub use atlas::{Atlas, Slot};
pub use error::{Result, UiError};
pub use font::{Baked, FontOptions, Fonts, GlyphInfo};
pub use renderer::{UiRenderer, UiStats};
pub use screens::{
    ChatHud, ChatOptions, DebugOverlay, HeartKind, HudSprites, HudState, fade_alpha,
};
pub use sprites::{Scaling, SpriteId, SpriteInfo, SpriteQuad, Sprites};
pub use style::{Cell, Flags, FontId, Style};
pub use text::{Lang, flatten, parse_legacy, push_legacy, wrap};
pub use ui::{Batch, DrawCmd, LINE_HEIGHT, PageKind, PageRef, Ui, UiFrame, Vertex, gui_scale};

pub use wgpu;
