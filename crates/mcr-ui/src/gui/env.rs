use mcr_protocol::play::clientbound::ui::{
    PositionedAdvancement, RecipeBookEntry, RecipeBookSettings, RecipeDisplay, SlotDisplay,
};
use mcr_protocol::types::component::HolderSet;
use mcr_protocol::types::item::ItemStack;
use mcr_protocol::types::text::Component;
use rustc_hash::FxHashMap;

use super::assets::ScreenAssets;
use super::tooltip::TooltipLines;
use crate::items::{ItemIcons, StackView};
use crate::sprites::SpriteId;
use crate::ui::UiFrame;

pub trait Clipboard {
    fn get(&mut self) -> String;
    fn set(&mut self, text: &str);
}

#[derive(Debug, Default)]
pub struct MemoryClipboard(pub String);

impl Clipboard for MemoryClipboard {
    fn get(&mut self) -> String {
        self.0.clone()
    }

    fn set(&mut self, text: &str) {
        self.0 = text.to_owned();
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TradeOffer {
    pub base_cost_a: ItemStack,
    pub cost_a: ItemStack,
    pub cost_b: ItemStack,
    pub result: ItemStack,
    pub out_of_stock: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MerchantInfo {
    pub offers: Vec<TradeOffer>,
    pub level: i32,
    pub xp: i32,
    pub future_xp: i32,
    pub show_progress: bool,
    pub can_restock: bool,
}

/// A map's colour bytes (128x128, row-major) with its saved data flags.
#[derive(Clone, Copy, Debug)]
pub struct MapView<'a> {
    pub colors: &'a [u8],
    pub scale: i8,
    pub locked: bool,
}

/// One of the player's active mob effects; `duration` is in ticks, -1 for infinite.
#[derive(Clone, Debug, PartialEq)]
pub struct ActiveEffect {
    pub effect: String,
    pub amplifier: i32,
    pub duration: i32,
    pub ambient: bool,
}

/// Live game state the screens read; the app implements it over its own state.
pub trait GameView {
    fn slot(&self, _index: usize) -> Option<&ItemStack> {
        None
    }

    /// Fraction of the item cooldown overlay drawn over `stack`.
    fn cooldown(&self, _stack: &ItemStack) -> f32 {
        0.0
    }

    fn carried(&self) -> Option<&ItemStack> {
        None
    }

    fn offhand(&self) -> Option<&ItemStack> {
        None
    }

    /// Container data property `index` (furnace progress, enchant costs, beacon levels, ...).
    fn data(&self, _index: usize) -> i32 {
        0
    }

    fn creative(&self) -> bool {
        false
    }

    fn spectator(&self) -> bool {
        false
    }

    fn screen_keys(&self) -> super::keys::ScreenKeys {
        super::keys::ScreenKeys::default()
    }

    /// `Player#mayBuild`: false in adventure and spectator mode.
    fn may_build(&self) -> bool {
        true
    }

    /// The player lies in a bed; the in-bed chat screen closes once this turns false.
    fn sleeping(&self) -> bool {
        false
    }

    fn xp_level(&self) -> i32 {
        0
    }

    fn merchant(&self) -> Option<&MerchantInfo> {
        None
    }

    fn stonecutter_options(&self) -> &[ItemStack] {
        &[]
    }

    fn loom_pattern_count(&self) -> usize {
        0
    }

    fn book_pages(&self) -> Vec<Component> {
        Vec::new()
    }

    /// A stack of the named item (`minecraft:` prefix optional) for server-provided displays.
    fn item_stack(&self, _id: &str, _count: i32) -> Option<ItemStack> {
        None
    }

    /// Clue for enchanting option `_option`: the enchantment's translation key, level and max level.
    fn enchant_clue(&self, _option: usize) -> Option<(String, i32, i32)> {
        None
    }

    /// The full tooltip of `stack`; the app builds it from its registries and caches it per hovered stack.
    fn item_tooltip(&self, _stack: &ItemStack) -> Option<TooltipLines> {
        None
    }

    fn trim_materials(&self) -> &[String] {
        &[]
    }

    fn map(&self, _id: i32) -> Option<MapView<'_>> {
        None
    }

    fn active_effects(&self) -> Vec<ActiveEffect> {
        Vec::new()
    }

    fn tick_rate(&self) -> f32 {
        20.0
    }

    fn score(&self) -> i32 {
        0
    }

    fn hardcore(&self) -> bool {
        false
    }

    fn single_player(&self) -> bool {
        false
    }

    /// Feature flags the server enabled, such as `minecraft:vanilla`.
    fn enabled_features(&self) -> &[String] {
        &[]
    }

    /// Whether the creative operator-items tab applies: operator in creative with the option on.
    fn operator_tab(&self) -> bool {
        false
    }

    /// Item tags whose id contains `query` (or `namespace:path` parts) with their item ids.
    fn item_tags_matching(&self, _query: &str) -> Vec<(String, Vec<i32>)> {
        Vec::new()
    }

    /// Bumped whenever recipes are added or removed or the book settings change.
    fn recipe_book_revision(&self) -> u64 {
        0
    }

    fn recipe_entries(&self) -> Vec<&RecipeBookEntry> {
        Vec::new()
    }

    fn recipe_settings(&self) -> RecipeBookSettings {
        RecipeBookSettings::default()
    }

    /// Every stack a slot display cycles through.
    fn resolve_display(&self, _display: &SlotDisplay) -> Vec<ItemStack> {
        Vec::new()
    }

    fn holder_items(&self, _set: &HolderSet) -> Vec<i32> {
        Vec::new()
    }

    /// The server's outline for the open crafting grid with a counter that changes per packet.
    fn ghost_recipe(&self) -> Option<(u64, &RecipeDisplay)> {
        None
    }

    fn advancements_revision(&self) -> u64 {
        0
    }

    fn advancement_defs(&self) -> Vec<&PositionedAdvancement> {
        Vec::new()
    }

    /// Criterion name to the instant it was obtained, when it was.
    fn advancement_progress(&self, _id: &str) -> Option<&FxHashMap<String, Option<i64>>> {
        None
    }

    fn advancement_selected_tab(&self) -> Option<&str> {
        None
    }

    /// Zero until the server answered a stats request.
    fn stats_revision(&self) -> u64 {
        0
    }

    fn stat(&self, _stat_type: i32, _id: i32) -> i32 {
        0
    }
}

#[derive(Debug, Default)]
pub struct NoView;

impl GameView for NoView {}

pub fn is_empty(stack: Option<&ItemStack>) -> bool {
    stack.is_none_or(ItemStack::is_empty)
}

impl std::fmt::Debug for Env<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Env").field("now_ms", &self.now_ms).finish()
    }
}

impl std::fmt::Debug for RenderCx<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RenderCx")
            .field("mouse", &self.mouse)
            .finish()
    }
}

pub struct Env<'a> {
    pub view: &'a dyn GameView,
    pub clipboard: &'a mut dyn Clipboard,
    pub now_ms: u64,
}

pub struct RenderCx<'a, 'f> {
    pub frame: &'a mut UiFrame<'f>,
    pub icons: &'a mut ItemIcons,
    pub assets: &'a ScreenAssets,
    pub view: &'a dyn GameView,
    pub mouse: (f32, f32),
    pub now_ms: u64,
    pub tooltip: Option<(TooltipLines, f32, f32)>,
    /// In: the host can draw the title panorama behind the GUI.
    pub panorama_loaded: bool,
    /// Out: the screen relies on that panorama this frame.
    pub panorama_drawn: bool,
}

impl<'a, 'f> RenderCx<'a, 'f> {
    pub fn new(
        frame: &'a mut UiFrame<'f>,
        icons: &'a mut ItemIcons,
        assets: &'a ScreenAssets,
        view: &'a dyn GameView,
    ) -> Self {
        Self {
            frame,
            icons,
            assets,
            view,
            mouse: (0.0, 0.0),
            now_ms: 0,
            tooltip: None,
            panorama_loaded: false,
            panorama_drawn: false,
        }
    }

    pub fn gui_size(&self) -> (i32, i32) {
        self.frame.gui_size()
    }

    pub fn sprite(&mut self, name: &str, x: i32, y: i32, w: i32, h: i32) {
        if let Some(id) = self.sprite_id(name) {
            self.frame.sprite(id, x, y, w, h);
        }
    }

    /// A GUI sprite, falling back to the whole-image textures such as mob effect icons.
    pub fn sprite_id(&mut self, name: &str) -> Option<SpriteId> {
        self.frame
            .ui()
            .sprite_id(name)
            .or_else(|| self.assets.texture(name))
    }

    pub fn texture(&mut self, name: &str, (u, v): (i32, i32), rect: (i32, i32, i32, i32)) {
        if let Some(id) = self.assets.texture(name) {
            self.frame.sprite_region(id, (u, v), rect);
        }
    }

    /// Draws the `sw`x`sh` window of a texture at `(u, v)` stretched over the destination rectangle.
    pub fn texture_scaled(
        &mut self,
        name: &str,
        src: (i32, i32, i32, i32),
        dst: (i32, i32, i32, i32),
    ) {
        if let Some(id) = self.assets.texture(name) {
            self.frame.sprite_region_scaled(id, src, dst);
        }
    }

    pub fn text_width(&mut self, text: &str) -> i32 {
        self.frame.ui().text_width(text).ceil() as i32
    }

    pub fn text(&mut self, text: &str, x: i32, y: i32, argb: u32, shadow: bool) {
        self.frame.text(text, x as f32, y as f32, argb, shadow);
    }

    pub fn centered_text(&mut self, text: &str, cx: i32, y: i32, argb: u32, shadow: bool) {
        let w = self.frame.ui().text_width(text);
        self.frame
            .text(text, (cx as f32 - w / 2.0).floor(), y as f32, argb, shadow);
    }

    fn lang_raw(&mut self, key: &str) -> String {
        self.frame
            .ui()
            .lang()
            .get(key)
            .map_or_else(|| key.to_owned(), str::to_owned)
    }

    pub fn lang(&mut self, key: &str) -> String {
        self.lang_raw(key).replace("%%", "%")
    }

    pub fn lang_fmt(&mut self, key: &str, args: &[&str]) -> String {
        let mut out = self.lang_raw(key);
        for (i, arg) in args.iter().enumerate() {
            out = out
                .replace(&format!("%{}$s", i + 1), arg)
                .replace(&format!("%{}$d", i + 1), arg);
        }
        for arg in args {
            if let Some(pos) = out.find("%s").or_else(|| out.find("%d")) {
                out.replace_range(pos..pos + 2, arg);
            }
        }
        out.replace("%%", "%")
    }

    pub fn stack(&mut self, stack: &ItemStack, x: i32, y: i32) {
        let Some(view) = StackView::new(stack) else {
            return;
        };
        let view = view.with_trim_materials(self.view.trim_materials());
        let item = view.view(self.view.cooldown(stack));
        self.icons.draw_with_decorations(self.frame, &item, x, y);
    }

    /// A stack drawn without its count or durability, like vanilla's `fakeItem`.
    pub fn fake_item(&mut self, stack: &ItemStack, x: i32, y: i32) {
        self.fake_item_sized(stack, x, y, 16);
    }

    pub fn fake_item_sized(&mut self, stack: &ItemStack, x: i32, y: i32, size: i32) {
        let Some(view) = StackView::new(stack) else {
            return;
        };
        let view = view.with_trim_materials(self.view.trim_materials());
        let item = view.view(0.0);
        self.icons.draw_sized(self.frame, &item, x, y, size, size);
    }

    pub fn stack_opt(&mut self, stack: Option<&ItemStack>, x: i32, y: i32) {
        if let Some(stack) = stack {
            self.stack(stack, x, y);
        }
    }

    pub fn item_tooltip(&mut self, stack: &ItemStack) -> TooltipLines {
        if let Some(lines) = self.view.item_tooltip(stack) {
            return lines;
        }
        let flags = crate::items::TooltipFlags {
            advanced: false,
            creative: self.view.creative(),
        };
        super::tooltip::item_tooltip(stack, &crate::items::NoTooltipSource, flags)
    }

    pub fn set_tooltip(&mut self, lines: TooltipLines) {
        let (x, y) = self.mouse;
        self.tooltip = Some((lines, x, y));
    }

    pub fn finish(&mut self) {
        if let Some((lines, x, y)) = self.tooltip.take() {
            super::tooltip::draw_tooltip(self, &lines, x as i32, y as i32);
        }
    }
}
