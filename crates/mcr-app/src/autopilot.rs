use std::collections::VecDeque;
use std::path::PathBuf;

use mcr_settings::InputKey;

use crate::shell::Shell;
use mcr_ui::{Key, MouseButton};

#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    Wait(u32),
    WaitReady,
    Hold(Vec<InputKey>, u32),
    Tap(InputKey),
    Press(Vec<InputKey>),
    Release(Vec<InputKey>),
    Look { yaw: f32, pitch: f32 },
    Turn { yaw: f32, pitch: f32 },
    Chat(String),
    Type(String),
    UiKey(Key),
    Click { x: f32, y: f32, right: bool },
    SlotClick { slot: usize, right: bool },
    Sleep(u32),
    Slot(u8),
    Shot(PathBuf),
    Report,
    Quit,
}

pub fn key_by_name(name: &str) -> Option<InputKey> {
    let short = match name {
        "lmb" | "attack" => "key.mouse.left",
        "rmb" | "use" => "key.mouse.right",
        "mmb" | "pick" => "key.mouse.middle",
        "shift" => "key.keyboard.left.shift",
        "ctrl" => "key.keyboard.left.control",
        "enter" => "key.keyboard.enter",
        other => {
            return InputKey::from_name(&format!("key.keyboard.{other}"))
                .filter(|k| *k != InputKey::Unbound);
        }
    };
    InputKey::from_name(short)
}

pub fn parse(text: &str) -> Result<Vec<Step>, String> {
    let mut steps = Vec::new();
    for (n, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let err = |m: &str| format!("autopilot line {}: {m}", n + 1);
        let (cmd, rest) = line.split_once(' ').unwrap_or((line, ""));
        let rest = rest.trim();
        let num = |s: &str| s.parse::<f32>().map_err(|_| err("bad number"));
        steps.push(match cmd {
            "wait" => Step::Wait(rest.parse().map_err(|_| err("wait needs ticks"))?),
            "wait_ready" => Step::WaitReady,
            "hold" => {
                let (keys, ticks) = rest
                    .rsplit_once(' ')
                    .ok_or_else(|| err("hold <keys> <ticks>"))?;
                let keys = keys
                    .split('+')
                    .map(|k| key_by_name(k.trim()).ok_or_else(|| err("unknown key")))
                    .collect::<Result<Vec<_>, _>>()?;
                Step::Hold(keys, ticks.trim().parse().map_err(|_| err("bad ticks"))?)
            }
            "press" | "release" => {
                let keys = rest
                    .split('+')
                    .map(|k| key_by_name(k.trim()).ok_or_else(|| err("unknown key")))
                    .collect::<Result<Vec<_>, _>>()?;
                if cmd == "press" {
                    Step::Press(keys)
                } else {
                    Step::Release(keys)
                }
            }
            "tap" => Step::Tap(key_by_name(rest).ok_or_else(|| err("unknown key"))?),
            "look" | "turn" => {
                let (a, b) = rest.split_once(' ').ok_or_else(|| err("needs yaw pitch"))?;
                let (yaw, pitch) = (num(a)?, num(b.trim())?);
                if cmd == "look" {
                    Step::Look { yaw, pitch }
                } else {
                    Step::Turn { yaw, pitch }
                }
            }
            "chat" => Step::Chat(rest.to_owned()),
            "type" => Step::Type(rest.to_owned()),
            "ui" => Step::UiKey(match rest {
                "enter" => Key::Enter,
                "escape" => Key::Escape,
                "tab" => Key::Tab,
                "backspace" => Key::Backspace,
                "up" => Key::Up,
                "down" => Key::Down,
                "left" => Key::Left,
                "right" => Key::Right,
                "space" => Key::Space,
                _ => return Err(err("unknown ui key")),
            }),
            "click" => {
                let mut it = rest.split_whitespace();
                let x = num(it.next().ok_or_else(|| err("click x y"))?)?;
                let y = num(it.next().ok_or_else(|| err("click x y"))?)?;
                Step::Click {
                    x,
                    y,
                    right: it.next() == Some("right"),
                }
            }
            "slotclick" => {
                let mut it = rest.split_whitespace();
                Step::SlotClick {
                    slot: it
                        .next()
                        .and_then(|v| v.parse().ok())
                        .ok_or_else(|| err("slotclick N [right]"))?,
                    right: it.next() == Some("right"),
                }
            }
            "sleep" => Step::Sleep(rest.parse().map_err(|_| err("sleep needs ticks"))?),
            "slot" => Step::Slot(rest.parse().map_err(|_| err("slot 1-9"))?),
            "shot" => Step::Shot(PathBuf::from(rest)),
            "report" => Step::Report,
            "quit" => Step::Quit,
            _ => return Err(err("unknown command")),
        });
    }
    Ok(steps)
}

/// Scripted input driven through the same entry points as the keyboard and mouse.
#[derive(Debug)]
pub struct Autopilot {
    steps: VecDeque<Step>,
    waiting: u32,
    holding: Vec<InputKey>,
    pub finished: bool,
}

impl Autopilot {
    pub fn new(steps: Vec<Step>) -> Self {
        Self {
            steps: steps.into(),
            waiting: 0,
            holding: Vec::new(),
            finished: false,
        }
    }

    /// Runs at the start of every tick.
    pub fn step(&mut self, sh: &mut Shell) {
        if self.waiting > 0 {
            self.waiting -= 1;
            if self.waiting == 0 {
                for key in self.holding.drain(..) {
                    sh.key_event(key, false, None);
                }
            }
            if self.waiting > 0 || !self.holding.is_empty() {
                return;
            }
        }
        while let Some(step) = self.steps.pop_front() {
            match step {
                Step::Wait(ticks) | Step::Sleep(ticks) => {
                    self.waiting = ticks.max(1);
                    return;
                }
                Step::WaitReady => {
                    if !sh.ready() {
                        self.steps.push_front(Step::WaitReady);
                        return;
                    }
                }
                Step::Hold(keys, ticks) => {
                    for &key in &keys {
                        sh.key_event(key, true, None);
                    }
                    self.holding = keys;
                    self.waiting = ticks.max(1);
                    return;
                }
                Step::Press(keys) => {
                    for key in keys {
                        sh.key_event(key, true, None);
                    }
                }
                Step::Release(keys) => {
                    for key in keys {
                        sh.key_event(key, false, None);
                    }
                }
                Step::Tap(key) => {
                    sh.key_event(key, true, None);
                    sh.key_event(key, false, None);
                }
                Step::Look { yaw, pitch } => {
                    if let Some(play) = &mut sh.play {
                        play.yaw = yaw.rem_euclid(360.0);
                        play.pitch = pitch.clamp(-90.0, 90.0);
                    }
                }
                Step::Turn { yaw, pitch } => {
                    if let Some(play) = &mut sh.play {
                        play.look(yaw, pitch);
                    }
                }
                Step::Chat(text) => {
                    if let Some(play) = &mut sh.play {
                        play.submit_chat(&text);
                    }
                }
                Step::Type(text) => sh.ui_text(&text),
                Step::UiKey(key) => sh.ui_key(key),
                Step::Click { x, y, right } => sh.screen_click(
                    x,
                    y,
                    if right {
                        MouseButton::Right
                    } else {
                        MouseButton::Left
                    },
                ),
                Step::SlotClick { slot, right } => {
                    if let Some(play) = &mut sh.play {
                        let button = if right {
                            MouseButton::Right
                        } else {
                            MouseButton::Left
                        };
                        if !play.click_slot(slot, button) {
                            println!("slotclick {slot}: no such slot on screen");
                        }
                    }
                }
                Step::Slot(n) if (1..=9).contains(&n) => {
                    if let Some(key) = InputKey::from_name(&format!("key.keyboard.{n}")) {
                        sh.key_event(key, true, None);
                        sh.key_event(key, false, None);
                    }
                }
                Step::Slot(_) => {}
                Step::Shot(path) => {
                    sh.shot = Some(path);
                    self.waiting = 2;
                    return;
                }
                Step::Report => {
                    if let Some(play) = &sh.play {
                        let p = play.feet();
                        println!(
                            "report: tick {} pos {:.2} {:.2} {:.2} yaw {:.1} pitch {:.1} teleports {} chunks {} screen {} container_updates {}",
                            play.ticks,
                            p[0],
                            p[1],
                            p[2],
                            play.yaw,
                            play.pitch,
                            play.world.client.state.player.teleports_received,
                            play.world.client.state.chunks.len(),
                            play.screen_open(),
                            play.container_updates,
                        );
                    }
                }
                Step::Quit => {
                    self.finished = true;
                    return;
                }
            }
        }
        self.finished = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_script() {
        let steps =
            parse("wait 5\nhold w+shift 20 # go\nlook 90 -10\nchat /time set day\nshot a.png\n")
                .unwrap();
        assert_eq!(steps.len(), 5);
        assert!(matches!(&steps[1], Step::Hold(keys, 20) if keys.len() == 2));
        assert_eq!(steps[3], Step::Chat("/time set day".into()));
        assert!(parse("hold nonsense 3").is_err());
        assert!(parse("dance").is_err());
    }
}
