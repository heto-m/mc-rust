use crate::ui::{LINE_HEIGHT, UiFrame};

const TEXT_COLOR: u32 = 0xFFE0_E0E0;
const BACKGROUND: u32 = 0x9050_5050;

/// F3 overlay data: lines may carry `§` formatting; empty lines keep their row but draw nothing.
#[derive(Clone, Debug, Default)]
pub struct DebugOverlay {
    pub left: Vec<String>,
    pub right: Vec<String>,
}

impl DebugOverlay {
    pub fn draw(&self, frame: &mut UiFrame<'_>) {
        let (gui_width, _) = frame.gui_size();
        for (lines, align_left) in [(&self.left, true), (&self.right, false)] {
            let placed: Vec<_> = lines
                .iter()
                .enumerate()
                .filter(|(_, l)| !l.is_empty())
                .map(|(i, line)| {
                    let width = frame.ui().text_width(line).ceil() as i32;
                    let left = if align_left { 2 } else { gui_width - 2 - width };
                    (line, left, 2 + LINE_HEIGHT * i as i32, width)
                })
                .collect();
            for &(_, left, top, width) in &placed {
                frame.fill_rect(
                    left - 1,
                    top - 1,
                    left + width + 1,
                    top + LINE_HEIGHT - 1,
                    BACKGROUND,
                );
            }
            for &(line, left, top, _) in &placed {
                frame.text(line, left as f32, top as f32, TEXT_COLOR, false);
            }
        }
    }
}
