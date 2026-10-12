//! Scripted input through `ModInput`, the same calls mcr's window events become.

use std::time::{Duration, Instant};

use mcr_client::Client;
use mcr_modbridge::ModInput;
use mcr_modbridge::input::us_printable;

use super::DEADLINE;
use super::compose::{Rig, Widget};

pub const LEFT: u8 = 1;
pub const ESCAPE: u32 = 41;
pub const ENTER: u32 = 40;
pub const LEFT_CTRL: u32 = 224;
pub const LEFT_SHIFT: u32 = 225;

/// The scancode a US layout types `c` with, for letters, digits and space.
pub fn scancode(c: char) -> u32 {
    if c == ' ' {
        return 44;
    }
    (4..57)
        .find(|&s| us_printable(s) == Some(c.to_ascii_lowercase()))
        .unwrap_or_else(|| panic!("no key for {c:?}"))
}

/// The composing rig plus, once joined, Rust's client running exactly the ticks Java is given.
pub struct Live {
    pub rig: Rig,
    pub client: Option<Client>,
    pub input: ModInput,
    pos: (f64, f64),
}

impl Live {
    pub fn new(rig: Rig) -> Self {
        let input = rig.bridge.input();
        Self {
            rig,
            client: None,
            input,
            pos: (0.0, 0.0),
        }
    }

    pub fn frame(&mut self) {
        let ticks = self.rig.frame();
        if let Some(client) = &mut self.client {
            for _ in 0..ticks {
                let input = client.idle_input();
                client.tick(&input).expect("client tick");
            }
        }
    }

    pub fn settle(&mut self, frames: u32) {
        for _ in 0..frames {
            self.frame();
        }
    }

    pub fn run_for(&mut self, d: Duration) {
        let start = Instant::now();
        while start.elapsed() < d {
            self.frame();
        }
    }

    pub fn until(&mut self, what: &str, mut done: impl FnMut(&mut Self) -> bool) {
        let start = Instant::now();
        while !done(self) {
            assert!(start.elapsed() < DEADLINE, "timed out waiting for {what}");
            self.frame();
        }
    }

    pub fn move_to(&mut self, x: f64, y: f64) {
        self.input.motion(x, y, x - self.pos.0, y - self.pos.1);
        self.pos = (x, y);
        self.settle(2);
    }

    pub fn click_at(&mut self, x: f64, y: f64, button: u8) {
        self.move_to(x, y);
        self.input.button(button, true);
        self.settle(2);
        self.input.button(button, false);
        self.settle(3);
    }

    pub fn click(&mut self, w: Widget) {
        self.click_at(
            f64::from(w.x) + f64::from(w.w) / 2.0,
            f64::from(w.y) + f64::from(w.h) / 2.0,
            LEFT,
        );
    }

    pub fn tap(&mut self, scancode: u32) {
        self.input
            .key(scancode, us_printable(scancode), true, false);
        self.settle(2);
        self.input
            .key(scancode, us_printable(scancode), false, false);
        self.settle(2);
    }

    /// Key presses with their text events, as a window with text input on delivers them.
    pub fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            let s = scancode(c);
            self.input.key(s, us_printable(s), true, false);
            self.input.text(&c.to_string());
            self.input.key(s, us_printable(s), false, false);
            self.settle(1);
        }
        self.settle(2);
    }

    pub fn chord(&mut self, modifier: u32, scancode: u32) {
        self.input.key(modifier, None, true, false);
        self.tap(scancode);
        self.input.key(modifier, None, false, false);
        self.settle(2);
    }

    pub fn wait_info(&mut self, key: &str, want: &str) {
        self.until(&format!("{key}={want}"), |l| {
            l.rig.infos.get(key).is_some_and(|v| v == want)
        });
    }

    pub fn wait_screen(&mut self, class_part: &str) {
        self.until(&format!("a screen matching {class_part}"), |l| {
            l.rig.screen.contains(class_part)
        });
        self.run_for(Duration::from_millis(300));
    }

    pub fn button(&self, label: &str) -> Widget {
        let layout = self
            .rig
            .infos
            .get("screen_layout")
            .cloned()
            .unwrap_or_default();
        super::compose::widget(&layout, label)
            .unwrap_or_else(|| panic!("no {label} button in {layout}"))
    }

    /// Text fields of the current screen from the bridge's `edit_boxes` probe, in pixels.
    pub fn edit_boxes(&self) -> Vec<Widget> {
        let Some(report) = self.rig.infos.get("edit_boxes") else {
            return Vec::new();
        };
        let mut parts = report.split('|');
        let scale: u32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(1);
        parts
            .filter_map(|p| {
                let f: Vec<u32> = p.split(',').filter_map(|v| v.parse().ok()).collect();
                (f.len() == 4).then(|| Widget {
                    x: f[0] * scale,
                    y: f[1] * scale,
                    w: f[2] * scale,
                    h: f[3] * scale,
                })
            })
            .collect()
    }
}
