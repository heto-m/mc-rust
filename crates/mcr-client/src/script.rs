//! Tiny line-based behaviour scripts driving a [`Client`] without a renderer.
//!
//! Lines: `wait <secs>`, `wait_loaded`, `walk <yaw> <ticks> [sprint]`, `square <side_ticks>`, `jump`,
//! `look <yaw> <pitch>`, `chat <text>`, `cmd <command>`, `dig <x> <y> <z>`,
//! `place <x> <y> <z> <down|up|north|south|west|east>`, `inventory`, `respawn`, `slot <0-8>`,
//! `report`. `#` starts a comment.

use std::collections::VecDeque;

use mcr_buf::BlockPos;
use mcr_physics::PlayerInput;
use mcr_protocol::play::serverbound::Facing;

use crate::client::Client;
use crate::error::Result;

#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    Wait(u32),
    WaitLoaded,
    Walk { yaw: f32, ticks: u32, sprint: bool },
    Square { side_ticks: u32 },
    Jump,
    Look { yaw: f32, pitch: f32 },
    Chat(String),
    Command(String),
    Dig(BlockPos),
    Place(BlockPos, Facing),
    Inventory,
    Respawn,
    Slot(u8),
    Report,
}

fn facing(name: &str) -> Option<Facing> {
    Some(match name {
        "down" => Facing::Down,
        "up" => Facing::Up,
        "north" => Facing::North,
        "south" => Facing::South,
        "west" => Facing::West,
        "east" => Facing::East,
        _ => return None,
    })
}

pub fn parse(text: &str) -> std::result::Result<Vec<Step>, String> {
    let mut steps = Vec::new();
    for (n, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let err = |m: &str| format!("line {}: {m}: {line}", n + 1);
        let (word, rest) = line.split_once(' ').unwrap_or((line, ""));
        let args: Vec<&str> = rest.split_whitespace().collect();
        let num = |i: usize| -> std::result::Result<f32, String> {
            args.get(i)
                .and_then(|a| a.parse().ok())
                .ok_or_else(|| err("bad number"))
        };
        let int = |i: usize| -> std::result::Result<i32, String> {
            args.get(i)
                .and_then(|a| a.parse().ok())
                .ok_or_else(|| err("bad integer"))
        };
        steps.push(match word {
            "wait" => Step::Wait((num(0)? * 20.0).clamp(0.0, 1.0e6) as u32),
            "wait_loaded" => Step::WaitLoaded,
            "walk" => Step::Walk {
                yaw: num(0)?,
                ticks: num(1)? as u32,
                sprint: args.get(2) == Some(&"sprint"),
            },
            "square" => Step::Square {
                side_ticks: num(0)? as u32,
            },
            "jump" => Step::Jump,
            "look" => Step::Look {
                yaw: num(0)?,
                pitch: num(1)?,
            },
            "chat" => Step::Chat(rest.to_owned()),
            "cmd" => Step::Command(rest.to_owned()),
            "dig" => Step::Dig(BlockPos::new(int(0)?, int(1)?, int(2)?)),
            "place" => Step::Place(
                BlockPos::new(int(0)?, int(1)?, int(2)?),
                args.get(3)
                    .and_then(|f| facing(f))
                    .ok_or_else(|| err("bad face"))?,
            ),
            "inventory" => Step::Inventory,
            "respawn" => Step::Respawn,
            "slot" => Step::Slot(int(0)?.clamp(0, 8) as u8),
            "report" => Step::Report,
            _ => return Err(err("unknown step")),
        });
    }
    Ok(steps)
}

#[derive(Debug)]
enum Active {
    Timed {
        remaining: u32,
        forward: bool,
        sprint: bool,
        jump_first: bool,
    },
    Square {
        side_ticks: u32,
        remaining: u32,
        side: u32,
    },
    Dig {
        remaining: u32,
    },
}

#[derive(Debug, Default)]
pub struct Runner {
    steps: VecDeque<Step>,
    active: Option<Active>,
    yaw: f32,
    pitch: f32,
    pub log: Vec<String>,
}

impl Runner {
    pub fn new(steps: Vec<Step>) -> Self {
        Self {
            steps: steps.into(),
            ..Self::default()
        }
    }

    pub fn is_done(&self) -> bool {
        self.steps.is_empty() && self.active.is_none()
    }

    fn input(&self, forward: bool, sprint: bool, jump: bool) -> PlayerInput {
        PlayerInput {
            forward,
            sprint,
            jump,
            yaw: self.yaw,
            pitch: self.pitch,
            ..PlayerInput::default()
        }
    }

    /// Returns the keys for this tick, advancing the script; instant steps run back to back.
    pub fn tick(&mut self, client: &mut Client) -> Result<PlayerInput> {
        loop {
            match self.active.take() {
                Some(Active::Timed {
                    remaining,
                    forward,
                    sprint,
                    jump_first,
                }) => {
                    if remaining == 0 {
                        continue;
                    }
                    self.active = Some(Active::Timed {
                        remaining: remaining - 1,
                        forward,
                        sprint,
                        jump_first: false,
                    });
                    return Ok(self.input(forward, sprint, jump_first));
                }
                Some(Active::Square {
                    side_ticks,
                    remaining,
                    side,
                }) => {
                    if remaining == 0 {
                        if side + 1 >= 4 {
                            continue;
                        }
                        self.yaw += 90.0;
                        self.active = Some(Active::Square {
                            side_ticks,
                            remaining: side_ticks,
                            side: side + 1,
                        });
                        continue;
                    }
                    self.active = Some(Active::Square {
                        side_ticks,
                        remaining: remaining - 1,
                        side,
                    });
                    return Ok(self.input(true, false, false));
                }
                Some(Active::Dig { remaining }) => {
                    if remaining == 0 {
                        client.finish_digging()?;
                        continue;
                    }
                    self.active = Some(Active::Dig {
                        remaining: remaining - 1,
                    });
                    return Ok(self.input(false, false, false));
                }
                None => {}
            }
            let Some(step) = self.steps.front().cloned() else {
                return Ok(self.input(false, false, false));
            };
            if step == Step::WaitLoaded {
                if client.state.player.loaded {
                    self.steps.pop_front();
                    continue;
                }
                return Ok(self.input(false, false, false));
            }
            self.steps.pop_front();
            match step {
                Step::Wait(t) => {
                    self.active = Some(Active::Timed {
                        remaining: t,
                        forward: false,
                        sprint: false,
                        jump_first: false,
                    })
                }
                Step::WaitLoaded => {}
                Step::Walk { yaw, ticks, sprint } => {
                    self.yaw = yaw;
                    self.active = Some(Active::Timed {
                        remaining: ticks,
                        forward: true,
                        sprint,
                        jump_first: false,
                    });
                }
                Step::Square { side_ticks } => {
                    self.active = Some(Active::Square {
                        side_ticks,
                        remaining: side_ticks,
                        side: 0,
                    });
                }
                Step::Jump => {
                    self.active = Some(Active::Timed {
                        remaining: 20,
                        forward: false,
                        sprint: false,
                        jump_first: true,
                    })
                }
                Step::Look { yaw, pitch } => {
                    self.yaw = yaw;
                    self.pitch = pitch.clamp(-90.0, 90.0);
                }
                Step::Chat(text) => client.send_chat(&text)?,
                Step::Command(text) => client.send_command(&text)?,
                Step::Dig(pos) => match client.estimate_dig_ticks(pos) {
                    Some(ticks) => {
                        client.start_digging(pos, Facing::Up)?;
                        self.active = Some(Active::Dig { remaining: ticks });
                    }
                    None => self.log.push(format!("cannot dig {pos:?}")),
                },
                Step::Place(pos, face) => client.place_block(pos, face)?,
                Step::Inventory => {
                    client.open_inventory()?;
                    let items = client
                        .state
                        .inventory
                        .player
                        .slots
                        .iter()
                        .filter(|s| !s.is_empty())
                        .count();
                    self.log.push(format!("inventory: {items} non-empty slots"));
                }
                Step::Respawn => client.respawn()?,
                Step::Slot(s) => client.select_hotbar(s)?,
                Step::Report => {
                    let p = &client.state.player;
                    self.log.push(format!(
                        "pos {:.2} {:.2} {:.2} yaw {:.0} ground {} health {} chunks {}",
                        p.physics.pos[0],
                        p.physics.pos[1],
                        p.physics.pos[2],
                        p.physics.yaw,
                        p.physics.on_ground,
                        p.health,
                        client.state.chunks.len(),
                    ));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_steps_and_skips_comments() {
        let steps =
            parse("# hi\nwait 1.5\nwalk 90 40 sprint\nplace 1 2 3 up\nchat hello world\n").unwrap();
        assert_eq!(steps[0], Step::Wait(30));
        assert_eq!(
            steps[1],
            Step::Walk {
                yaw: 90.0,
                ticks: 40,
                sprint: true
            }
        );
        assert_eq!(steps[2], Step::Place(BlockPos::new(1, 2, 3), Facing::Up));
        assert_eq!(steps[3], Step::Chat("hello world".into()));
    }

    #[test]
    fn rejects_bad_lines() {
        assert!(parse("fly away").is_err());
        assert!(parse("place 1 2 3 sideways").is_err());
        assert!(parse("walk x y").is_err());
    }
}
