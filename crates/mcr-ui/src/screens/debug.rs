use crate::ui::{LINE_HEIGHT, UiFrame};

const TEXT_COLOR: u32 = 0xFFE0_E0E0;
const BACKGROUND: u32 = 0x9050_5050;

/// F3 overlay data: lines may carry `§` formatting; empty lines keep their row but draw nothing.
#[derive(Clone, Debug, Default)]
pub struct DebugOverlay {
    pub left: Vec<String>,
    pub right: Vec<String>,
    pub charts: Vec<DebugChart>,
    /// The 16x16 level lightmap (sky by block light), drawn bottom right.
    pub lightmap: Option<Vec<[f32; 4]>>,
}

pub const CHART_SAMPLES: usize = 240;
const CHART_HEIGHT: i32 = 60;

/// A rolling bar chart along the bottom edge, one bar per sample, newest on the right.
#[derive(Clone, Debug, Default)]
pub struct DebugChart {
    pub title: String,
    pub unit: &'static str,
    pub samples: Vec<f32>,
    /// Value drawn at full height.
    pub scale: f32,
    /// Bars above these turn yellow, then red.
    pub warn: f32,
    pub bad: f32,
}

impl DebugChart {
    fn color(&self, v: f32) -> u32 {
        if v > self.bad {
            0xFFD0_3030
        } else if v > self.warn {
            0xFFD0_C030
        } else {
            0xFF30_D030
        }
    }

    /// Bar heights in pixels for the newest `CHART_SAMPLES` samples.
    pub fn bar_heights(&self) -> Vec<i32> {
        let scale = self.scale.max(f32::MIN_POSITIVE);
        self.samples
            .iter()
            .rev()
            .take(CHART_SAMPLES)
            .rev()
            .map(|v| {
                let v = if v.is_finite() { v.max(0.0) } else { 0.0 };
                ((v / scale) * CHART_HEIGHT as f32)
                    .ceil()
                    .min(CHART_HEIGHT as f32) as i32
            })
            .collect()
    }

    fn draw(&self, frame: &mut UiFrame<'_>, left: i32, width: i32, bottom: i32) {
        let heights = self.bar_heights();
        let top = bottom - CHART_HEIGHT;
        frame.fill_rect(left, top, left + width, bottom, BACKGROUND);
        let shown = heights.len().min(width.max(0) as usize);
        let first = width - shown as i32;
        let newest = self.samples.iter().rev().take(shown).rev();
        let heights = &heights[heights.len() - shown..];
        for (i, (h, v)) in heights.iter().zip(newest).enumerate() {
            let x = left + first + i as i32;
            frame.fill_rect(x, bottom - h, x + 1, bottom, self.color(*v));
        }
        for (value, color) in [(self.warn, 0xFFFF_FFFF), (self.bad, 0xFFFF_6060)] {
            let y =
                bottom - ((value / self.scale.max(f32::MIN_POSITIVE)) * CHART_HEIGHT as f32) as i32;
            if (top..bottom).contains(&y) {
                frame.fill_rect(left, y, left + width, y + 1, color & 0x80FF_FFFF);
            }
        }
        let latest = self.samples.last().copied().unwrap_or(0.0);
        let label = format!("{} {:.0} {}", self.title, latest, self.unit);
        frame.text(
            &label,
            (left + 2) as f32,
            (top + 2) as f32,
            TEXT_COLOR,
            true,
        );
    }
}

/// Vanilla's places: the first chart bottom-left, the rest bottom-right, stacked upwards.
fn chart_slot(index: usize, width: i32, gui_width: i32, gui_height: i32) -> (i32, i32) {
    match index {
        0 => (0, gui_height),
        n => (
            gui_width - width,
            gui_height - (n as i32 - 1) * (CHART_HEIGHT + 2),
        ),
    }
}

impl DebugOverlay {
    pub fn draw(&self, frame: &mut UiFrame<'_>) {
        let (gui_width, gui_height) = frame.gui_size();
        let width = (gui_width / 2).min(CHART_SAMPLES as i32);
        for (i, chart) in self.charts.iter().enumerate() {
            let (left, bottom) = chart_slot(i, width, gui_width, gui_height);
            chart.draw(frame, left, width, bottom);
        }
        if let Some(map) = &self.lightmap {
            let (x, y) = (gui_width - 66, gui_height - 66);
            frame.fill_rect(x - 1, y - 1, x + 65, y + 65, 0xFF00_0000);
            for (i, texel) in map.iter().take(256).enumerate() {
                let (col, row) = ((i % 16) as i32, (i / 16) as i32);
                let [r, g, b] = [texel[0], texel[1], texel[2]]
                    .map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u32);
                let (cx, cy) = (x + col * 4, y + 60 - row * 4);
                frame.fill_rect(cx, cy, cx + 4, cy + 4, 0xFF00_0000 | r << 16 | g << 8 | b);
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn charts_sit_bottom_left_then_bottom_right() {
        assert_eq!(chart_slot(0, 240, 960, 540), (0, 540));
        assert_eq!(chart_slot(1, 240, 960, 540), (720, 540));
        assert_eq!(chart_slot(2, 240, 960, 540), (720, 540 - CHART_HEIGHT - 2));
    }

    #[test]
    fn bars_scale_clamp_and_keep_only_the_newest_samples() {
        let chart = DebugChart {
            scale: 50.0,
            warn: 16.7,
            bad: 33.3,
            samples: vec![0.0, 25.0, 50.0, 500.0, f32::NAN, -3.0],
            ..DebugChart::default()
        };
        assert_eq!(chart.bar_heights(), [0, 30, 60, 60, 0, 0]);
        let long = DebugChart {
            scale: 1.0,
            samples: vec![1.0; CHART_SAMPLES * 3],
            ..DebugChart::default()
        };
        assert_eq!(long.bar_heights().len(), CHART_SAMPLES);
        assert_eq!(chart.color(10.0), 0xFF30_D030);
        assert_eq!(chart.color(20.0), 0xFFD0_C030);
        assert_eq!(chart.color(40.0), 0xFFD0_3030);
    }
}
