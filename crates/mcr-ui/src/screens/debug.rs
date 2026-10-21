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
    /// The profiler pie of F3+1, bottom right above the charts.
    pub pie: Option<ProfilerPie>,
}

/// One section of the profiler pie.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PieSlice {
    pub name: String,
    /// Share of the root in percent.
    pub percent: f32,
    pub color: u32,
}

/// `ProfilerPieChart`: the root's name and its children, largest first.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProfilerPie {
    pub root: String,
    pub slices: Vec<PieSlice>,
}

const PIE_HALF_WIDTH: i32 = 130;
const PIE_HALF_HEIGHT: i32 = 52;
const PIE_THICKNESS: i32 = 10;
const PIE_PANEL: u32 = 0x9050_5050;
const MAX_PIE_SLICES: usize = 32;

/// The slice index of each pixel run on the pie's row `dy` (relative to its centre): `(x0, x1, slice)`.
fn pie_runs(rx: i32, ry: i32, dy: i32, cumulative: &[f32]) -> Vec<(i32, i32, usize)> {
    let v = dy as f32 / ry.max(1) as f32;
    if v.abs() > 1.0 || cumulative.is_empty() {
        return Vec::new();
    }
    let half = (rx as f32 * (1.0 - v * v).sqrt()) as i32;
    let mut runs: Vec<(i32, i32, usize)> = Vec::new();
    for dx in -half..half {
        let angle = (dx as f32 / rx.max(1) as f32).atan2(-v);
        let fraction = angle.rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU;
        let slice = cumulative
            .iter()
            .position(|c| fraction < *c)
            .unwrap_or(cumulative.len() - 1);
        match runs.last_mut() {
            Some(last) if last.2 == slice => last.1 = dx + 1,
            _ => runs.push((dx, dx + 1, slice)),
        }
    }
    runs
}

fn darker(argb: u32) -> u32 {
    let [a, r, g, b] = argb.to_be_bytes();
    u32::from_be_bytes([a, r / 2, g / 2, b / 2])
}

impl ProfilerPie {
    /// Slice boundaries as fractions of the full circle, from the slices' shares.
    fn cumulative(&self) -> Vec<f32> {
        let total: f32 = self.slices.iter().map(|s| s.percent.max(0.0)).sum();
        let mut sum = 0.0;
        self.slices
            .iter()
            .map(|s| {
                sum += s.percent.max(0.0) / total.max(f32::MIN_POSITIVE);
                sum
            })
            .collect()
    }

    fn draw(&self, frame: &mut UiFrame<'_>, gui_width: i32, gui_height: i32, bottom_offset: i32) {
        let slices = &self.slices[..self.slices.len().min(MAX_PIE_SLICES)];
        let center_x = gui_width - 140;
        let (left, right) = (center_x - PIE_HALF_WIDTH, center_x + PIE_HALF_WIDTH);
        let bottom = gui_height - bottom_offset - 5;
        let text_top = bottom - slices.len() as i32 * 9;
        let center_y = text_top - 62 - 5;
        let title_top = center_y - 62;
        frame.fill_rect(left - 5, title_top - 5, right + 5, bottom + 5, PIE_PANEL);
        let pie = ProfilerPie {
            root: String::new(),
            slices: slices.to_vec(),
        };
        let cumulative = pie.cumulative();
        let ring_y = center_y + 5;
        let wall = [PIE_THICKNESS, 7, 4, 2];
        for shift in wall.into_iter().chain([0]) {
            for dy in -PIE_HALF_HEIGHT..PIE_HALF_HEIGHT {
                for (x0, x1, slice) in pie_runs(PIE_HALF_WIDTH, PIE_HALF_HEIGHT, dy, &cumulative) {
                    let color = slices[slice].color;
                    let color = if shift == 0 { color } else { darker(color) };
                    let y = ring_y + dy + shift;
                    frame.fill_rect(center_x + x0, y, center_x + x1, y + 1, color);
                }
            }
        }
        let percent = |p: f32| format!("{p:.2}%");
        let total = percent(100.0);
        let total_width = frame.ui().text_width(&total).ceil() as i32;
        frame.text(
            &self.root,
            left as f32,
            title_top as f32,
            0xFFFF_FFFF,
            false,
        );
        frame.text(
            &total,
            (right - total_width) as f32,
            title_top as f32,
            0xFFFF_FFFF,
            false,
        );
        for (i, slice) in slices.iter().enumerate() {
            let y = (text_top + i as i32 * 9) as f32;
            frame.text(
                &format!("[{}] {}", i + 1, slice.name),
                left as f32,
                y,
                slice.color,
                false,
            );
            let text = percent(slice.percent);
            let width = frame.ui().text_width(&text).ceil() as i32;
            for column in [right - 50, right] {
                frame.text(&text, (column - width) as f32, y, slice.color, false);
            }
        }
    }
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
        if let Some(pie) = &self.pie {
            let offset = (self.charts.len().saturating_sub(1) as i32) * (CHART_HEIGHT + 2);
            pie.draw(frame, gui_width, gui_height, offset);
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
    fn pie_rows_split_by_angle_from_twelve_o_clock() {
        let cumulative = [0.5, 1.0];
        let top = pie_runs(100, 50, -25, &cumulative);
        assert_eq!(
            top.first().map(|r| r.2),
            Some(1),
            "left of the top is the last slice"
        );
        assert_eq!(
            top.last().map(|r| r.2),
            Some(0),
            "right of the top is the first"
        );
        let split = top.iter().find(|r| r.2 == 0).unwrap().0;
        assert!(
            (-2..=2).contains(&split),
            "the boundary sits at the vertical axis: {split}"
        );
        let bottom = pie_runs(100, 50, 25, &cumulative);
        assert_eq!(
            bottom.first().map(|r| r.2),
            Some(1),
            "the second half closes the circle on the left"
        );
        assert_eq!(bottom.last().map(|r| r.2), Some(0));
        assert!(pie_runs(100, 50, 60, &cumulative).is_empty());
        assert!(pie_runs(100, 50, 0, &[]).is_empty());
    }

    #[test]
    fn pie_boundaries_follow_the_shares() {
        let pie = ProfilerPie {
            root: "root".into(),
            slices: vec![
                PieSlice {
                    name: "a".into(),
                    percent: 30.0,
                    color: 0,
                },
                PieSlice {
                    name: "b".into(),
                    percent: 70.0,
                    color: 0,
                },
            ],
        };
        let c = pie.cumulative();
        assert!((c[0] - 0.3).abs() < 1e-6 && (c[1] - 1.0).abs() < 1e-6);
        assert!(ProfilerPie::default().cumulative().is_empty());
    }

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
