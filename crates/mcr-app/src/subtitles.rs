//! Vanilla `SubtitleOverlay`: the captions of recent sounds, bottom right, with an arrow towards
//! sounds out of view, fading over three seconds (scaled by the notification time option).

use mcr_protocol::types::Component;
use mcr_settings::Options;
use mcr_ui::UiFrame;

use crate::audio::SubtitleCue;

const DISPLAY_MS: f64 = 3000.0;
const LOOPING_FULL_MS: u64 = 500;
const MAX_ENTRIES: usize = 64;
const MAX_PLACES: usize = 16;

#[derive(Debug, Clone)]
struct Entry {
    key: String,
    range: f32,
    looping: bool,
    played: Vec<([f64; 3], u64)>,
}

/// One caption to draw: its key, where it is relative to the view, and its grey level.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub key: String,
    pub rightness: f64,
    pub forwardness: f64,
    pub brightness: u8,
}

#[derive(Debug, Default)]
pub struct Subtitles {
    entries: Vec<Entry>,
}

fn dist_sq(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| (a[i] - b[i]) * (a[i] - b[i])).sum()
}

impl Entry {
    /// `Subtitle#getBestSubtitleCandidate`: the latest place in range, nearest first on ties.
    fn best(&self, listener: [f64; 3]) -> Option<([f64; 3], u64)> {
        if self.played.len() == 1 {
            return self.played.first().copied();
        }
        let range = f64::from(self.range);
        self.played
            .iter()
            .filter(|(at, _)| dist_sq(*at, listener) < range * range)
            .max_by(|a, b| {
                a.1.cmp(&b.1)
                    .then(dist_sq(a.0, listener).total_cmp(&dist_sq(b.0, listener)))
            })
            .copied()
    }

    fn audible(&self, listener: [f64; 3]) -> bool {
        if self.range.is_infinite() {
            return !self.played.is_empty();
        }
        let range = f64::from(self.range);
        self.best(listener)
            .is_some_and(|(at, _)| dist_sq(at, listener) < range * range)
    }

    fn brightness(&self, played_at: u64, now: u64, multiplier: f64) -> u8 {
        let full = if self.looping { LOOPING_FULL_MS } else { 0 };
        let age = now.saturating_sub(played_at).saturating_sub(full) as f64;
        let t = (age / ((DISPLAY_MS - full as f64) * multiplier)).clamp(0.0, 1.0);
        (255.0 + (75.0 - 255.0) * t).floor() as u8
    }
}

impl Subtitles {
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// `SubtitleOverlay#onPlaySound`: the same caption is refreshed instead of listed twice.
    pub fn push(&mut self, cue: SubtitleCue, now: u64) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.key == cue.key) {
            e.played.retain(|(at, _)| *at != cue.pos);
            if e.played.len() >= MAX_PLACES {
                e.played.remove(0);
            }
            e.played.push((cue.pos, now));
            return;
        }
        if self.entries.len() >= MAX_ENTRIES {
            self.entries.remove(0);
        }
        self.entries.push(Entry {
            key: cue.key,
            range: cue.range,
            looping: cue.looping,
            played: vec![(cue.pos, now)],
        });
    }

    /// The captions to show now, oldest first; expired places are dropped.
    pub fn lines(
        &mut self,
        listener: [f64; 3],
        forward: [f64; 3],
        right: [f64; 3],
        now: u64,
        multiplier: f64,
    ) -> Vec<Line> {
        let max_age = DISPLAY_MS * multiplier;
        for e in &mut self.entries {
            e.played
                .retain(|(_, at)| (now.saturating_sub(*at) as f64) <= max_age);
        }
        self.entries.retain(|e| !e.played.is_empty());
        self.entries
            .iter()
            .filter(|e| e.audible(listener))
            .filter_map(|e| {
                let (at, played) = e.best(listener)?;
                let d: [f64; 3] = std::array::from_fn(|i| at[i] - listener[i]);
                let len = dist_sq(at, listener).sqrt();
                let n = if len > 1.0e-9 {
                    d.map(|v| v / len)
                } else {
                    [0.0; 3]
                };
                let dot = |v: [f64; 3]| v[0] * n[0] + v[1] * n[1] + v[2] * n[2];
                Some(Line {
                    key: e.key.clone(),
                    rightness: dot(right),
                    forwardness: dot(forward),
                    brightness: e.brightness(played, now, multiplier),
                })
            })
            .collect()
    }
}

/// Draws `lines` the way `SubtitleOverlay#extractRenderState` lays them out.
pub fn draw(frame: &mut UiFrame<'_>, lines: &[Line], options: &Options) {
    if lines.is_empty() {
        return;
    }
    let texts: Vec<_> = lines
        .iter()
        .map(|l| {
            frame
                .ui()
                .flatten(&Component::translatable(l.key.clone(), Vec::new()))
        })
        .collect();
    let mut width = texts
        .iter()
        .map(|t| frame.ui().width(t))
        .fold(0.0f32, f32::max);
    let glyph = |frame: &mut UiFrame<'_>, s: &str| {
        let cells = frame.ui().flatten(&Component::text(s));
        frame.ui().width(&cells)
    };
    let (lt, gt, space) = (glyph(frame, "<"), glyph(frame, ">"), glyph(frame, " "));
    width += lt + space + gt + space;
    let half_w = (width as i32) / 2;
    let (gui_w, gui_h) = frame.gui_size();
    let opacity = if options.background_for_chat_only {
        0.8
    } else {
        options.text_background_opacity.clamp(0.0, 1.0)
    };
    let background = ((opacity * 255.0) as u32) << 24;
    for (row, (line, text)) in lines.iter().zip(&texts).enumerate() {
        let cx = gui_w as f32 - half_w as f32 - 2.0;
        let cy = gui_h as f32 - 35.0 - row as f32 * 10.0;
        let (x, y) = (cx as i32, cy as i32);
        frame.fill_rect(x - half_w - 1, y - 5, x + half_w + 1, y + 5, background);
        let b = u32::from(line.brightness);
        let color = 0xFF00_0000 | b << 16 | b << 8 | b;
        let top = cy - 4.0;
        if line.forwardness <= 0.5 {
            if line.rightness > 0.0 {
                frame.text(">", cx + half_w as f32 - gt, top, color, true);
            } else if line.rightness < 0.0 {
                frame.text("<", cx - half_w as f32, top, color, true);
            }
        }
        let w = frame.ui().width(text);
        frame.cells(text, cx - (w as i32 / 2) as f32, top, color, true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cue(key: &str, pos: [f64; 3], range: f32) -> SubtitleCue {
        SubtitleCue {
            key: key.to_owned(),
            pos,
            range,
            looping: false,
        }
    }

    const FWD: [f64; 3] = [0.0, 0.0, 1.0];
    const RIGHT: [f64; 3] = [-1.0, 0.0, 0.0];

    #[test]
    fn captions_merge_fade_and_expire() {
        let mut s = Subtitles::default();
        s.push(
            cue("subtitles.entity.pig.ambient", [0.0, 0.0, 5.0], 16.0),
            0,
        );
        s.push(
            cue("subtitles.entity.pig.ambient", [0.0, 0.0, 6.0], 16.0),
            100,
        );
        s.push(
            cue("subtitles.block.chest.open", [-5.0, 0.0, 0.0], 16.0),
            100,
        );
        let lines = s.lines([0.0; 3], FWD, RIGHT, 100, 1.0);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].brightness, 255);
        assert!(lines[0].forwardness > 0.9);
        assert!(
            lines[1].rightness > 0.9,
            "west is to the right when facing south"
        );
        let faded = s.lines([0.0; 3], FWD, RIGHT, 1600, 1.0);
        assert!(faded[0].brightness < 255 && faded[0].brightness > 75);
        assert!(s.lines([0.0; 3], FWD, RIGHT, 3200, 1.0).is_empty());
        assert!(s.entries.is_empty());
    }

    #[test]
    fn far_sounds_are_not_listed_and_global_ones_always_are() {
        let mut s = Subtitles::default();
        s.push(cue("a", [100.0, 0.0, 0.0], 16.0), 0);
        s.push(cue("b", [100.0, 0.0, 0.0], f32::INFINITY), 0);
        let lines = s.lines([0.0; 3], FWD, RIGHT, 10, 1.0);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].key, "b");
    }

    #[test]
    fn caption_lists_stay_bounded() {
        let mut s = Subtitles::default();
        for i in 0..10_000 {
            s.push(
                cue(&format!("k{}", i % 100), [f64::from(i), 0.0, 0.0], 16.0),
                0,
            );
        }
        assert!(s.entries.len() <= MAX_ENTRIES);
        assert!(s.entries.iter().all(|e| e.played.len() <= MAX_PLACES));
    }
}
