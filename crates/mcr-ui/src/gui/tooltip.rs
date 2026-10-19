use mcr_protocol::types::component::DataComponent;
use mcr_protocol::types::item::ItemStack;
use mcr_protocol::types::text::Component;

use super::env::RenderCx;
use crate::items::{
    BundleWeight, ItemRef, TooltipFlags, TooltipSource, bundle_items, bundle_weight, items_to_show,
    styled_hover_name, tooltip_lines,
};
use crate::style::Cell;
use crate::text::parse_legacy;

const MAX_LINES: usize = 64;
const MAX_LINE_CHARS: usize = 512;
const MAX_LINE_CELLS: usize = 1024;
const BUNDLE_WIDTH: i32 = 96;
const BUNDLE_SLOT: i32 = 24;
const BUNDLE_BAR_HEIGHT: i32 = 13;
const BUNDLE_BAR_FILL: u64 = 94;

/// A bundle's contents drawn under the tooltip's first line (`ClientBundleTooltip`).
#[derive(Clone, Debug, PartialEq)]
pub struct BundleImage {
    pub items: Vec<ItemStack>,
    /// `None` when vanilla's weight computation fails; nothing is drawn then.
    pub weight: Option<BundleWeight>,
    pub selected: Option<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TooltipLine {
    Legacy(String),
    Component(Component),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TooltipLines {
    lines: Vec<TooltipLine>,
    /// The `tooltip_style` id whose `tooltip/<path>_background` and `_frame` sprites replace the default.
    pub style: Option<String>,
    pub bundle: Option<BundleImage>,
}

impl TooltipLines {
    pub fn text(text: impl Into<String>) -> Self {
        let mut lines = Self::default();
        lines.push_text(text);
        lines
    }

    pub fn push_text(&mut self, text: impl Into<String>) {
        if self.lines.len() < MAX_LINES {
            let text: String = text.into();
            self.lines.push(TooltipLine::Legacy(
                text.chars().take(MAX_LINE_CHARS).collect(),
            ));
        }
    }

    pub fn insert_text(&mut self, index: usize, text: impl Into<String>) {
        if self.lines.len() < MAX_LINES {
            let text: String = text.into();
            self.lines.insert(
                index.min(self.lines.len()),
                TooltipLine::Legacy(text.chars().take(MAX_LINE_CHARS).collect()),
            );
        }
    }

    pub fn push_component(&mut self, component: Component) {
        if self.lines.len() < MAX_LINES {
            self.lines.push(TooltipLine::Component(component));
        }
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    pub fn first_plain(&self) -> String {
        match self.lines.first() {
            Some(TooltipLine::Legacy(s)) => parse_legacy(s).iter().map(|c| c.ch).collect(),
            Some(TooltipLine::Component(c)) => c.to_plain_string(),
            None => String::new(),
        }
    }

    /// The first line as plain text with translations resolved, like `getHoverName().getString()`.
    pub fn first_plain_in(&self, lang: &crate::text::Lang) -> String {
        match self.lines.first() {
            Some(TooltipLine::Component(c)) => {
                crate::text::flatten(lang, c).iter().map(|c| c.ch).collect()
            }
            _ => self.first_plain(),
        }
    }

    pub fn lines(&self) -> &[TooltipLine] {
        &self.lines
    }

    /// Every line as plain text with translations resolved, for searches.
    pub fn plain_lines(&self, lang: &crate::text::Lang) -> Vec<String> {
        self.lines
            .iter()
            .map(|line| match line {
                TooltipLine::Legacy(s) => parse_legacy(s).iter().map(|c| c.ch).collect(),
                TooltipLine::Component(c) => {
                    crate::text::flatten(lang, c).iter().map(|c| c.ch).collect()
                }
            })
            .collect()
    }

    pub fn select_bundle_item(&mut self, selected: Option<usize>) {
        if let Some(bundle) = &mut self.bundle {
            bundle.selected = selected.filter(|&i| i < bundle.items.len());
        }
    }
}

/// The full vanilla tooltip of a stack: its lines, `tooltip_style` and the bundle contents image.
pub fn item_tooltip(
    stack: &ItemStack,
    src: &dyn TooltipSource,
    flags: TooltipFlags,
) -> TooltipLines {
    let item = ItemRef::of_stack(stack);
    let mut lines = TooltipLines::default();
    for line in tooltip_lines(&item, src, flags) {
        lines.push_component(line);
    }
    if lines.is_empty() {
        return lines;
    }
    if let Some(DataComponent::TooltipStyle(style)) = item.get("tooltip_style") {
        lines.style = Some(style.as_str().to_owned());
    }
    let hides_contents = match item.get("tooltip_display") {
        Some(DataComponent::TooltipDisplay(d)) => {
            d.hide_tooltip
                || mcr_protocol::types::component::component_type_id("bundle_contents")
                    .is_some_and(|id| d.hidden_components.iter().any(|h| h.0 == id))
        }
        _ => false,
    };
    if !hides_contents && let Some(contents) = bundle_items(stack) {
        lines.bundle = Some(BundleImage {
            items: contents
                .iter()
                .map(|t| ItemStack {
                    item: t.item,
                    count: t.count,
                    components: t.components.clone(),
                })
                .collect(),
            weight: bundle_weight(contents),
            selected: None,
        });
    }
    lines
}

fn bundle_rows(b: &BundleImage) -> i32 {
    (b.items.len().min(12) as i32 + 3) / 4
}

fn bundle_empty_lines(cx: &mut RenderCx<'_, '_>) -> Vec<Vec<Cell>> {
    let text = cx.lang("item.minecraft.bundle.empty.description");
    let cells = parse_legacy(&text);
    let ranges = cx.frame.ui().wrap(&cells, BUNDLE_WIDTH as f32);
    ranges.into_iter().map(|r| cells[r].to_vec()).collect()
}

fn bundle_height(cx: &mut RenderCx<'_, '_>, b: &BundleImage) -> i32 {
    if b.items.is_empty() {
        bundle_empty_lines(cx).len() as i32 * 9 + BUNDLE_BAR_HEIGHT + 8
    } else {
        bundle_rows(b) * BUNDLE_SLOT + BUNDLE_BAR_HEIGHT + 8
    }
}

fn draw_bundle_bar(cx: &mut RenderCx<'_, '_>, x: i32, y: i32, weight: BundleWeight) {
    let fill = weight.mul_truncate(BUNDLE_BAR_FILL).min(BUNDLE_BAR_FILL) as i32;
    let sprite = if weight.is_full() {
        "container/bundle/bundle_progressbar_full"
    } else {
        "container/bundle/bundle_progressbar_fill"
    };
    if fill > 0 {
        cx.sprite(sprite, x + 1, y, fill, BUNDLE_BAR_HEIGHT);
    }
    cx.sprite(
        "container/bundle/bundle_progressbar_border",
        x,
        y,
        BUNDLE_WIDTH,
        BUNDLE_BAR_HEIGHT,
    );
    let label = if weight.is_zero() {
        Some("item.minecraft.bundle.empty")
    } else if weight.is_full() {
        Some("item.minecraft.bundle.full")
    } else {
        None
    };
    if let Some(key) = label {
        let text = cx.lang(key);
        cx.centered_text(&text, x + BUNDLE_WIDTH / 2, y + 3, 0xFFFF_FFFF, true);
    }
}

fn draw_bundle(cx: &mut RenderCx<'_, '_>, b: &BundleImage, x: i32, y: i32, w: i32) {
    let Some(weight) = b.weight else {
        return;
    };
    let left = x + (w - BUNDLE_WIDTH) / 2;
    if b.items.is_empty() {
        let mut ty = y;
        for line in bundle_empty_lines(cx) {
            cx.frame
                .cells(&line, left as f32, ty as f32, 0xFFAA_AAAA, false);
            ty += 9;
        }
        draw_bundle_bar(cx, left, ty + 4, BundleWeight::ZERO);
        return;
    }
    let rows = bundle_rows(b);
    let overflowing = b.items.len() > 12;
    let shown = items_to_show(b.items.len()).min(b.items.len());
    let hidden: i64 = b.items[shown..]
        .iter()
        .map(|s| i64::from(s.count.max(0)))
        .sum();
    let (x_start, y_start) = (left + BUNDLE_WIDTH, y + rows * BUNDLE_SLOT);
    let mut slot = 1usize;
    for row in 1..=rows {
        for column in 1..=4 {
            let (dx, dy) = (x_start - column * BUNDLE_SLOT, y_start - row * BUNDLE_SLOT);
            if overflowing && column * row == 1 {
                cx.centered_text(&format!("+{hidden}"), dx + 12, dy + 10, 0xFFFF_FFFF, true);
            } else if shown >= slot {
                let index = shown - slot;
                let highlighted = b.selected == Some(index);
                let back = if highlighted {
                    "container/bundle/slot_highlight_back"
                } else {
                    "container/bundle/slot_background"
                };
                cx.sprite(back, dx, dy, BUNDLE_SLOT, BUNDLE_SLOT);
                cx.stack(&b.items[index], dx + 4, dy + 4);
                if highlighted {
                    cx.sprite(
                        "container/bundle/slot_highlight_front",
                        dx,
                        dy,
                        BUNDLE_SLOT,
                        BUNDLE_SLOT,
                    );
                }
                slot += 1;
            }
        }
    }
    if let Some(selected) = b.selected.and_then(|i| b.items.get(i)) {
        let name = styled_hover_name(&ItemRef::of_stack(selected), &crate::items::NoTooltipSource);
        let cells = cx.frame.ui().flatten(&name);
        let text_w = cx.frame.ui().width(&cells) as i32;
        let centre = x + w / 2 - 12;
        let mut lines = TooltipLines::default();
        lines.push_component(name);
        draw_lines(cx, &lines, centre - text_w / 2, y - 15);
    }
    draw_bundle_bar(cx, left, y + rows * BUNDLE_SLOT + 4, weight);
}

/// Vanilla's default tooltip positioner: 12 px right of and 12 px above the cursor, flipped or shifted to stay visible.
pub fn tooltip_position(screen: (i32, i32), mouse: (i32, i32), size: (i32, i32)) -> (i32, i32) {
    let (mut x, mut y) = (mouse.0 + 12, mouse.1 - 12);
    if x + size.0 > screen.0 {
        x = (x - 24 - size.0).max(4);
    }
    if y + size.1 + 3 > screen.1 {
        y = screen.1 - size.1 - 3;
    }
    (x, y)
}

pub fn draw_tooltip(cx: &mut RenderCx<'_, '_>, lines: &TooltipLines, mx: i32, my: i32) {
    draw_lines(cx, lines, mx, my);
}

fn style_sprites(cx: &mut RenderCx<'_, '_>, style: Option<&str>) -> (String, String) {
    if let Some(style) = style {
        let (namespace, path) = style.split_once(':').unwrap_or(("minecraft", style));
        let name = |suffix: &str| {
            if namespace == "minecraft" {
                format!("tooltip/{path}_{suffix}")
            } else {
                format!("{namespace}:tooltip/{path}_{suffix}")
            }
        };
        let (bg, frame) = (name("background"), name("frame"));
        if cx.sprite_id(&bg).is_some() && cx.sprite_id(&frame).is_some() {
            return (bg, frame);
        }
    }
    ("tooltip/background".to_owned(), "tooltip/frame".to_owned())
}

fn draw_lines(cx: &mut RenderCx<'_, '_>, lines: &TooltipLines, mx: i32, my: i32) {
    if lines.is_empty() {
        return;
    }
    let cells: Vec<Vec<Cell>> = lines
        .lines
        .iter()
        .map(|line| {
            let mut cells = match line {
                TooltipLine::Legacy(text) => parse_legacy(text),
                TooltipLine::Component(c) => cx.frame.ui().flatten(c),
            };
            cells.truncate(MAX_LINE_CELLS);
            cells
        })
        .collect();
    let bundle_h = lines.bundle.as_ref().map(|b| bundle_height(cx, b));
    let mut width = cells
        .iter()
        .map(|c| cx.frame.ui().width(c).ceil() as i32)
        .max()
        .unwrap_or(0);
    if bundle_h.is_some() {
        width = width.max(BUNDLE_WIDTH);
    }
    let components = cells.len() as i32 + i32::from(bundle_h.is_some());
    let height = if components == 1 {
        8
    } else {
        cells.len() as i32 * 10 + bundle_h.unwrap_or(0)
    };
    let (x, y) = tooltip_position(cx.gui_size(), (mx, my), (width, height));
    let (x0, y0) = (x - 12, y - 12);
    let (w0, h0) = (width + 24, height + 24);
    let (bg, frame) = style_sprites(cx, lines.style.as_deref());
    cx.sprite(&bg, x0, y0, w0, h0);
    cx.sprite(&frame, x0, y0, w0, h0);
    let mut ty = y;
    for (i, line) in cells.iter().enumerate() {
        cx.frame.cells(line, x as f32, ty as f32, 0xFFFF_FFFF, true);
        ty += if i == 0 && components > 1 { 12 } else { 10 };
        if i == 0
            && let (Some(b), Some(h)) = (&lines.bundle, bundle_h)
        {
            draw_bundle(cx, b, x, ty, width);
            ty += h;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tooltip_flips_at_right_edge() {
        let (x, _) = tooltip_position((320, 240), (310, 100), (60, 20));
        assert_eq!(x, 310 + 12 - 24 - 60);
        assert!(x + 60 <= 320);
    }

    #[test]
    fn tooltip_shifts_up_at_bottom() {
        let (_, y) = tooltip_position((320, 240), (10, 238), (60, 20));
        assert_eq!(y, 240 - 20 - 3);
    }

    #[test]
    fn tooltip_never_left_of_edge_margin() {
        let (x, _) = tooltip_position((100, 100), (95, 10), (200, 10));
        assert_eq!(x, 4);
    }
}
