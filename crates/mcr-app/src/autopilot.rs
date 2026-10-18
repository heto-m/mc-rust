use std::collections::VecDeque;
use std::path::PathBuf;

use mcr_settings::InputKey;

use crate::launcher::scene::Launcher;
use crate::shell::Shell;
use mcr_ui::{Key, MouseButton};

#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    Wait(u32),
    WaitReady,
    /// Until no session, connection or local server is left (e.g. after Save and Quit).
    WaitIdle,
    /// Until Java finished loading and its GUI is composed (mod mode).
    WaitMods,
    /// First step only: the script drives the menus itself instead of waiting for the join.
    Early,
    Hold(Vec<InputKey>, u32),
    Tap(InputKey),
    Press(Vec<InputKey>),
    Release(Vec<InputKey>),
    Look {
        yaw: f32,
        pitch: f32,
    },
    Turn {
        yaw: f32,
        pitch: f32,
    },
    Chat(String),
    Type(String),
    UiKey(Key),
    Click {
        x: f32,
        y: f32,
        right: bool,
    },
    Hover {
        x: f32,
        y: f32,
    },
    /// Press (`down`) or release (`up`) a button at GUI coordinates; `hover` between them drags.
    Button {
        x: f32,
        y: f32,
        right: bool,
        pressed: bool,
    },
    /// Wheel lines, positive away from the user.
    Wheel(f32),
    /// Relative mouse motion in window pixels, as a grabbed mouse reports it.
    Motion {
        dx: f64,
        dy: f64,
    },
    SlotClick {
        slot: usize,
        right: bool,
    },
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
            "wait_idle" => Step::WaitIdle,
            "wait_mods" => Step::WaitMods,
            "early" => Step::Early,
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
            "down" | "up" => {
                let mut it = rest.split_whitespace();
                let x = num(it.next().ok_or_else(|| err("down|up x y [right]"))?)?;
                let y = num(it.next().ok_or_else(|| err("down|up x y [right]"))?)?;
                Step::Button {
                    x,
                    y,
                    right: it.next() == Some("right"),
                    pressed: cmd == "down",
                }
            }
            "wheel" => Step::Wheel(num(rest)?),
            "motion" => {
                let (a, b) = rest.split_once(' ').ok_or_else(|| err("motion dx dy"))?;
                Step::Motion {
                    dx: f64::from(num(a)?),
                    dy: f64::from(num(b.trim())?),
                }
            }
            "hover" => {
                let (a, b) = rest.split_once(' ').ok_or_else(|| err("hover x y"))?;
                Step::Hover {
                    x: num(a)?,
                    y: num(b.trim())?,
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
            "shot" => Step::Shot(std::path::absolute(rest).unwrap_or_else(|_| rest.into())),
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

    /// Puts `step` before the remaining ones.
    pub fn prepend(&mut self, step: Step) {
        if self.steps.front() != Some(&Step::Early) {
            self.steps.push_front(step);
        }
    }

    /// Runs the UI steps against the launcher; any other step waits for the game (`true` once one does).
    pub fn step_launcher(&mut self, l: &mut Launcher) -> bool {
        if self.waiting > 0 {
            self.waiting -= 1;
            if self.waiting == 0 {
                for key in self.holding.drain(..) {
                    l.key_input(key, false, false, None);
                }
            }
            if self.waiting > 0 || !self.holding.is_empty() {
                return false;
            }
        }
        let button = |right: bool| {
            if right {
                MouseButton::Right
            } else {
                MouseButton::Left
            }
        };
        while let Some(step) = self.steps.pop_front() {
            match step {
                Step::Wait(ticks) | Step::Sleep(ticks) => {
                    self.waiting = ticks.max(1);
                    return false;
                }
                Step::Early => {}
                Step::Tap(key) => {
                    l.key_input(key, true, false, None);
                    l.key_input(key, false, false, None);
                }
                Step::Press(keys) => {
                    for key in keys {
                        l.key_input(key, true, false, None);
                    }
                }
                Step::Release(keys) => {
                    for key in keys {
                        l.key_input(key, false, false, None);
                    }
                }
                Step::Type(text) => l.ui_text(&text),
                Step::UiKey(key) => l.ui_key(key),
                Step::Click { x, y, right } => l.screen_button(x, y, button(right), None),
                Step::Hover { x, y } => l.screen_hover(x, y),
                Step::Button {
                    x,
                    y,
                    right,
                    pressed,
                } => l.screen_button(x, y, button(right), Some(pressed)),
                Step::Wheel(lines) => l.scroll(lines),
                Step::Shot(path) => {
                    l.shot = Some(path);
                    self.waiting = 2;
                    return false;
                }
                Step::Quit => {
                    self.finished = true;
                    return false;
                }
                other => {
                    self.steps.push_front(other);
                    return true;
                }
            }
        }
        self.finished = true;
        false
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
                Step::WaitIdle => {
                    if !sh.idle() {
                        self.steps.push_front(Step::WaitIdle);
                        return;
                    }
                }
                Step::WaitMods => {
                    if !sh.mods_settled() {
                        self.steps.push_front(Step::WaitMods);
                        return;
                    }
                }
                Step::Early => {}
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
                Step::Hover { x, y } => sh.screen_hover(x, y),
                Step::Button {
                    x,
                    y,
                    right,
                    pressed,
                } => sh.screen_button(
                    x,
                    y,
                    if right {
                        MouseButton::Right
                    } else {
                        MouseButton::Left
                    },
                    pressed,
                ),
                Step::Wheel(lines) => sh.wheel(0.0, f64::from(lines), lines),
                Step::Motion { dx, dy } => sh.mouse_motion(dx, dy),
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
                        let state = &play.world.client.state;
                        let [block, sky] = crate::entities::light_at(&state.chunks, p, 1.62);
                        let cam = play.camera(play.partial()).position;
                        let java_cam = play
                            .java_camera
                            .map_or("none", |c| if c.input { "captured" } else { "moved" });
                        println!(
                            "report: tick {} pos {:.2} {:.2} {:.2} yaw {:.1} pitch {:.1} light {block} {sky} teleports {} chunks {} screen {} container_updates {} corrections {} hotbar {:?} cursor {:?} mod input {:?} fov_scale {:.2} camera {:.2} {:.2} {:.2} java_camera {java_cam} java_roll {:.1} java_weather {} java_steps {} java_turn {} perspective {} view_roll {:.1} dyn_lights {} dead {} health {:.1} sleeping {} sleep_timer {} using {} miss {} strength {:.2} dig {:?} target {:?} eye {:.3}",
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
                            state.inventory.corrections,
                            (36..45)
                                .map(|i| {
                                    let s = &state.inventory.player.slots[i];
                                    (s.item, s.count)
                                })
                                .collect::<Vec<_>>(),
                            (state.inventory.cursor.item, state.inventory.cursor.count),
                            sh.mod_authority(),
                            play.java_fov_scale,
                            cam.x,
                            cam.y,
                            cam.z,
                            play.java_camera.map_or(0.0, |c| c.roll.to_degrees()),
                            play.java_weather,
                            play.java_steps,
                            play.java_turn,
                            play.perspective_name(),
                            play.java_view_roll.to_degrees(),
                            play.render.renderer.dynamic_lights_drawn(),
                            state.player.dead,
                            state.player.health,
                            state.player.is_sleeping(),
                            state.player.sleep_timer,
                            play.world.client.is_using_item(),
                            play.world.client.miss_time(),
                            play.world.client.attack_strength_scale(0.0),
                            play.world
                                .client
                                .digging()
                                .map(|d| (d.pos, d.ticks, d.progress)),
                            play.target,
                            state.player.physics.eye_height(),
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
            parse("wait 5\nhold w+shift 20 # go\nlook 90 -10\nchat /time set day\nshot a.png\nwait_mods\n")
                .unwrap();
        assert_eq!(steps.len(), 6);
        assert_eq!(steps[5], Step::WaitMods);
        assert!(matches!(&steps[1], Step::Hold(keys, 20) if keys.len() == 2));
        assert_eq!(steps[3], Step::Chat("/time set day".into()));
        assert_eq!(
            parse("hover 12 34.5").unwrap(),
            [Step::Hover { x: 12.0, y: 34.5 }]
        );
        assert!(parse("hover 12").is_err());
        assert_eq!(
            parse("down 1 2\nup 3 4 right\nwheel -1.5").unwrap(),
            [
                Step::Button {
                    x: 1.0,
                    y: 2.0,
                    right: false,
                    pressed: true
                },
                Step::Button {
                    x: 3.0,
                    y: 4.0,
                    right: true,
                    pressed: false
                },
                Step::Wheel(-1.5)
            ]
        );
        assert!(parse("down 1").is_err() && parse("wheel up").is_err());
        assert_eq!(
            parse("motion 200 -5").unwrap(),
            [Step::Motion {
                dx: 200.0,
                dy: -5.0
            }]
        );
        assert!(parse("motion 1").is_err());
        assert!(parse("hold nonsense 3").is_err());
        assert!(parse("dance").is_err());
    }
}
