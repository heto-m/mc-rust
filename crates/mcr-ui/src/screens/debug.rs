use crate::ui::{LINE_HEIGHT, UiFrame};

const TEXT_COLOR: u32 = 0xFFE0_E0E0;
const BACKGROUND: u32 = 0x9050_5050;

/// F3 overlay data: lines may carry `§` formatting; empty lines keep their row but draw nothing.
#[derive(Clone, Debug, Default)]
pub struct DebugOverlay {
    pub left: Vec<String>,
    pub right: Vec<String>,
    pub charts: Vec<DebugChart>,
}

pub const CHART_SAMPLES: usize = 240;
const CHART_HEIGHT: i32 = 60;

/// A rolling bar chart along the bottom-left edge, one bar per sample, newest on the right.
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

    fn draw(&self, frame: &mut UiFrame<'_>, left: i32, bottom: i32) {
        let heights = self.bar_heights();
        let width = CHART_SAMPLES as i32;
        let top = bottom - CHART_HEIGHT;
        frame.fill_rect(left, top, left + width, bottom, BACKGROUND);
        let first = width - heights.len() as i32;
        let newest = self.samples.iter().rev().take(CHART_SAMPLES).rev();
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

impl DebugOverlay {
    pub fn draw(&self, frame: &mut UiFrame<'_>) {
        let (gui_width, gui_height) = frame.gui_size();
        for (i, chart) in self.charts.iter().enumerate() {
            chart.draw(frame, 0, gui_height - i as i32 * (CHART_HEIGHT + 2));
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
