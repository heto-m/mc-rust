use mcr_protocol::types::text::Component;

use super::env::RenderCx;
use crate::style::Cell;
use crate::text::parse_legacy;

const MAX_LINES: usize = 64;
const MAX_LINE_CHARS: usize = 512;

#[derive(Clone, Debug, PartialEq)]
pub enum TooltipLine {
    Legacy(String),
    Component(Component),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TooltipLines {
    lines: Vec<TooltipLine>,
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

    pub fn lines(&self) -> &[TooltipLine] {
        &self.lines
    }
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
    if lines.is_empty() {
        return;
    }
    let cells: Vec<Vec<Cell>> = lines
        .lines
        .iter()
        .map(|line| match line {
            TooltipLine::Legacy(text) => parse_legacy(text),
            TooltipLine::Component(c) => cx.frame.ui().flatten(c),
        })
        .collect();
    let width = cells
        .iter()
        .map(|c| cx.frame.ui().width(c).ceil() as i32)
        .max()
        .unwrap_or(0);
    let n = cells.len() as i32;
    let height = if n == 1 { 8 } else { n * 10 };
    let (x, y) = tooltip_position(cx.gui_size(), (mx, my), (width, height));
    let (x0, y0) = (x - 12, y - 12);
    let (w0, h0) = (width + 24, height + 24);
    cx.sprite("tooltip/background", x0, y0, w0, h0);
    cx.sprite("tooltip/frame", x0, y0, w0, h0);
    let mut ty = y;
    for (i, line) in cells.iter().enumerate() {
        cx.frame.cells(line, x as f32, ty as f32, 0xFFFF_FFFF, true);
        ty += if i == 0 { 12 } else { 10 };
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
