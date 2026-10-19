use std::ops::Range;

use super::env::RenderCx;
use super::input::{MouseButton, UiInput, contains};

const PADDING: i32 = 4;
const SCROLLBAR_WIDTH: i32 = 6;
const WHEEL_STEP: f32 = 18.0;

/// A vertically scrolling list of fixed-height rows with a draggable scrollbar; the owner draws the rows.
#[derive(Clone, Debug)]
pub struct ScrollList {
    rect: [i32; 4],
    item_height: i32,
    count: usize,
    scroll: f32,
    dragging: bool,
}

impl ScrollList {
    pub fn new(rect: [i32; 4], item_height: i32) -> Self {
        Self {
            rect,
            item_height: item_height.max(1),
            count: 0,
            scroll: 0.0,
            dragging: false,
        }
    }

    pub fn rect(&self) -> [i32; 4] {
        self.rect
    }

    pub fn set_rect(&mut self, rect: [i32; 4]) {
        self.rect = rect;
        self.clamp();
    }

    pub fn set_count(&mut self, count: usize) {
        self.count = count;
        self.clamp();
    }

    pub fn scroll(&self) -> f32 {
        self.scroll
    }

    fn content_height(&self) -> i32 {
        self.count as i32 * self.item_height + 2 * PADDING
    }

    pub fn max_scroll(&self) -> f32 {
        (self.content_height() - self.rect[3]).max(0) as f32
    }

    fn clamp(&mut self) {
        self.scroll = self.scroll.clamp(0.0, self.max_scroll());
    }

    pub fn set_scroll(&mut self, scroll: f32) {
        self.scroll = scroll;
        self.clamp();
    }

    pub fn scroll_by(&mut self, pixels: f32) {
        self.set_scroll(self.scroll + pixels);
    }

    pub fn item_top(&self, index: usize) -> i32 {
        self.rect[1] + PADDING + index as i32 * self.item_height - self.scroll as i32
    }

    pub fn item_height(&self) -> i32 {
        self.item_height
    }

    pub fn visible_range(&self) -> Range<usize> {
        let first = ((self.scroll as i32 - PADDING).max(0) / self.item_height) as usize;
        let last = ((self.scroll as i32 + self.rect[3]) / self.item_height + 1) as usize;
        first.min(self.count)..last.min(self.count)
    }

    pub fn item_at(&self, x: f32, y: f32) -> Option<usize> {
        if !contains(self.rect, x, y) {
            return None;
        }
        let rel = y as i32 - self.rect[1] - PADDING + self.scroll as i32;
        if rel < 0 {
            return None;
        }
        let index = (rel / self.item_height) as usize;
        (index < self.count).then_some(index)
    }

    pub fn ensure_visible(&mut self, index: usize) {
        let top = (index as i32 * self.item_height) as f32;
        let bottom = top + (self.item_height + 2 * PADDING) as f32;
        if top < self.scroll {
            self.scroll = top;
        } else if bottom > self.scroll + self.rect[3] as f32 {
            self.scroll = bottom - self.rect[3] as f32;
        }
        self.clamp();
    }

    pub fn scrollbar_rect(&self, x: i32) -> Option<[i32; 4]> {
        (self.max_scroll() > 0.0).then_some([x, self.rect[1], SCROLLBAR_WIDTH, self.rect[3]])
    }

    fn thumb(&self) -> (i32, i32) {
        let height = self.rect[3];
        let thumb = (height * height / self.content_height().max(1)).clamp(32.min(height), height);
        let travel = (height - thumb) as f32;
        let offset = if self.max_scroll() > 0.0 {
            (travel * self.scroll / self.max_scroll()) as i32
        } else {
            0
        };
        (self.rect[1] + offset, thumb)
    }

    fn drag_to(&mut self, y: f32) {
        let (_, thumb) = self.thumb();
        let travel = (self.rect[3] - thumb).max(1) as f32;
        let t = (y - self.rect[1] as f32 - thumb as f32 / 2.0) / travel;
        self.set_scroll(t * self.max_scroll());
    }

    pub fn input(&mut self, input: &UiInput, bar_x: i32) -> bool {
        match *input {
            UiInput::Scroll { x, y, dy, .. } if contains(self.rect, x, y) => {
                self.scroll_by(-dy * WHEEL_STEP);
                true
            }
            UiInput::MousePress {
                x,
                y,
                button: MouseButton::Left,
                ..
            } => {
                if let Some(bar) = self.scrollbar_rect(bar_x)
                    && contains(bar, x, y)
                {
                    self.dragging = true;
                    self.drag_to(y);
                    return true;
                }
                false
            }
            UiInput::MouseMove { y, .. } if self.dragging => {
                self.drag_to(y);
                true
            }
            UiInput::MouseRelease { .. } => std::mem::take(&mut self.dragging),
            _ => false,
        }
    }

    pub fn render_scrollbar(&self, cx: &mut RenderCx<'_, '_>, bar_x: i32) {
        if self.scrollbar_rect(bar_x).is_none() {
            return;
        }
        cx.sprite(
            "widget/scroller_background",
            bar_x,
            self.rect[1],
            SCROLLBAR_WIDTH,
            self.rect[3],
        );
        let (y, h) = self.thumb();
        cx.sprite("widget/scroller", bar_x, y, SCROLLBAR_WIDTH, h);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(count: usize) -> ScrollList {
        let mut l = ScrollList::new([0, 33, 300, 100], 36);
        l.set_count(count);
        l
    }

    #[test]
    fn short_lists_do_not_scroll() {
        let mut l = list(2);
        assert_eq!(l.max_scroll(), 0.0);
        l.scroll_by(50.0);
        assert_eq!(l.scroll(), 0.0);
        assert!(l.scrollbar_rect(310).is_none());
    }

    #[test]
    fn scroll_clamps_to_content() {
        let mut l = list(10);
        assert_eq!(l.max_scroll(), (10 * 36 + 8 - 100) as f32);
        l.scroll_by(10_000.0);
        assert_eq!(l.scroll(), l.max_scroll());
        l.scroll_by(-10_000.0);
        assert_eq!(l.scroll(), 0.0);
    }

    #[test]
    fn items_map_to_rows_and_back() {
        let mut l = list(10);
        assert_eq!(l.item_at(10.0, (33 + 4 + 1) as f32), Some(0));
        assert_eq!(l.item_at(10.0, (33 + 4 + 36 + 1) as f32), Some(1));
        assert_eq!(l.item_at(10.0, 20.0), None);
        assert_eq!(l.item_at(400.0, 50.0), None);
        l.set_scroll(36.0);
        assert_eq!(l.item_at(10.0, (33 + 4 + 1) as f32), Some(1));
        assert_eq!(l.item_top(1), 33 + 4);
    }

    #[test]
    fn visible_range_follows_scroll() {
        let mut l = list(10);
        assert_eq!(l.visible_range(), 0..3);
        l.set_scroll(l.max_scroll());
        assert_eq!(l.visible_range().end, 10);
        assert!(l.visible_range().start >= 6);
    }

    #[test]
    fn ensure_visible_scrolls_minimally() {
        let mut l = list(10);
        l.ensure_visible(9);
        assert_eq!(l.scroll(), l.max_scroll());
        l.ensure_visible(0);
        assert_eq!(l.scroll(), 0.0);
        l.ensure_visible(1);
        assert_eq!(l.scroll(), 0.0);
    }

    #[test]
    fn wheel_and_scrollbar_drag() {
        let mut l = list(10);
        let wheel = UiInput::Scroll {
            x: 10.0,
            y: 60.0,
            dx: 0.0,
            dy: -1.0,
            mods: crate::gui::input::Modifiers::NONE,
        };
        assert!(l.input(&wheel, 310));
        assert_eq!(l.scroll(), 18.0);
        let outside = UiInput::Scroll {
            x: 10.0,
            y: 200.0,
            dx: 0.0,
            dy: -1.0,
            mods: crate::gui::input::Modifiers::NONE,
        };
        assert!(!l.input(&outside, 310));
        assert!(l.input(&UiInput::press(312.0, 130.0, MouseButton::Left), 310));
        assert_eq!(l.scroll(), l.max_scroll());
        assert!(l.input(&UiInput::MouseMove { x: 312.0, y: 0.0 }, 310));
        assert_eq!(l.scroll(), 0.0);
        assert!(l.input(&UiInput::release(312.0, 0.0, MouseButton::Left), 310));
        assert!(!l.input(&UiInput::MouseMove { x: 312.0, y: 100.0 }, 310));
    }
}
