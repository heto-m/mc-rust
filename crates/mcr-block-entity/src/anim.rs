use std::f32::consts::PI;

use crate::kind::BlockEntityKind;
use crate::place::Dir;

pub const BELL_TICKS: i32 = 50;
pub const MAX_CATCH_UP_TICKS: i64 = 40;
/// `BaseSpawner` default `minSpawnDelay`, which the spawn block event resets the delay to.
const SPAWNER_MIN_DELAY: i32 = 200;

fn lerp(t: f32, a: f32, b: f32) -> f32 {
    a + t * (b - a)
}

fn wrap_pi(mut a: f32) -> f32 {
    while a >= PI {
        a -= 2.0 * PI;
    }
    while a < -PI {
        a += 2.0 * PI;
    }
    a
}

/// Client-side per-tick state of one block entity, advanced once per game tick.
#[derive(Debug, Clone, Default)]
pub struct Anim {
    pub viewers: u8,
    pub open: f32,
    pub o_open: f32,
    pub bell_ticks: i32,
    pub bell_shaking: bool,
    pub bell_dir: Dir,
    pub wobble_start: i64,
    pub wobble_style: Option<u8>,
    pub spin: f32,
    pub o_spin: f32,
    pub delay: i32,
    pub age: i64,
    pub cooldown: i32,
    pub ticks: i32,
    pub book: Book,
    pub skull_ticks: i32,
}

#[derive(Debug, Clone, Default)]
pub struct Book {
    pub flip: f32,
    pub o_flip: f32,
    pub flip_t: f32,
    pub flip_a: f32,
    pub open: f32,
    pub o_open: f32,
    pub rot: f32,
    pub o_rot: f32,
    pub t_rot: f32,
    pub time: f32,
    pub rng: u32,
}

impl Anim {
    pub fn lid(&self, pt: f32) -> f32 {
        lerp(pt, self.o_open, self.open)
    }

    pub fn event(&mut self, kind: BlockEntityKind, b0: u8, b1: u8, game_time: i64) {
        use BlockEntityKind as K;
        match kind {
            K::Chest | K::TrappedChest | K::EnderChest | K::ShulkerBox if b0 == 1 => {
                self.viewers = b1;
            }
            K::Bell if b0 == 1 => {
                self.bell_dir = Dir::from_data3d(b1);
                self.bell_ticks = 0;
                self.bell_shaking = true;
            }
            K::DecoratedPot if b0 == 1 && b1 < 2 => {
                self.wobble_start = game_time;
                self.wobble_style = Some(b1);
            }
            K::EndGateway if b0 == 1 => self.cooldown = 40,
            K::Spawner if b0 == 1 => self.delay = SPAWNER_MIN_DELAY,
            _ => {}
        }
    }

    /// `BaseSpawner.clientTick`: the cage spins only with a player in range and a mob to show.
    pub fn tick_spawner(&mut self, active: bool) {
        self.o_spin = self.spin;
        if active {
            self.delay = (self.delay - 1).max(0);
            self.spin = (self.spin + 1000.0 / (self.delay as f32 + 200.0)) % 360.0;
        }
    }

    /// `TrialSpawner.tickClient`: the display mob spins only in the states with a spin speed.
    pub fn tick_trial_spawner(&mut self, state: &str, spawn_delay: i64) {
        let speed = match state {
            "waiting_for_players" => 200.0,
            "active" => 1000.0,
            _ => return,
        };
        self.o_spin = self.spin;
        self.spin = (self.spin + speed / (spawn_delay.max(0) as f32 + 200.0)) % 360.0;
    }

    pub fn tick(&mut self, kind: BlockEntityKind, powered: bool, player_near: Option<f32>) {
        use BlockEntityKind as K;
        match kind {
            K::Chest | K::TrappedChest | K::EnderChest | K::ShulkerBox => {
                self.o_open = self.open;
                if self.viewers == 0 && self.open > 0.0 {
                    self.open = (self.open - 0.1).max(0.0);
                } else if self.viewers > 0 && self.open < 1.0 {
                    self.open = (self.open + 0.1).min(1.0);
                }
            }
            K::Bell => {
                if self.bell_shaking {
                    self.bell_ticks += 1;
                    if self.bell_ticks >= BELL_TICKS {
                        self.bell_shaking = false;
                        self.bell_ticks = 0;
                    }
                }
            }
            K::Vault => {
                self.o_spin = self.spin;
                let s = self.spin + 10.0;
                self.spin = ((s + 180.0).rem_euclid(360.0)) - 180.0;
            }
            K::EndGateway => {
                self.age += 1;
                self.cooldown = (self.cooldown - 1).max(0);
            }
            K::Conduit => self.ticks += 1,
            K::Skull => {
                if powered {
                    self.skull_ticks += 1;
                }
            }
            K::EnchantingTable => self.book.tick(player_near),
            _ => {}
        }
    }
}

impl Book {
    fn next(&mut self) -> u32 {
        self.rng = self.rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        self.rng >> 16
    }

    pub fn tick(&mut self, player_angle: Option<f32>) {
        self.o_open = self.open;
        self.o_rot = self.rot;
        match player_angle {
            Some(angle) => {
                self.t_rot = angle;
                self.open += 0.1;
                if self.open < 0.5 || self.next().is_multiple_of(40) {
                    let old = self.flip_t;
                    loop {
                        self.flip_t += (self.next() % 4) as f32 - (self.next() % 4) as f32;
                        if self.flip_t != old {
                            break;
                        }
                    }
                }
            }
            None => {
                self.t_rot += 0.02;
                self.open -= 0.1;
            }
        }
        self.rot = wrap_pi(self.rot);
        self.t_rot = wrap_pi(self.t_rot);
        self.rot += wrap_pi(self.t_rot - self.rot) * 0.4;
        self.open = self.open.clamp(0.0, 1.0);
        self.time += 1.0;
        self.o_flip = self.flip;
        let diff = ((self.flip_t - self.flip) * 0.4).clamp(-0.2, 0.2);
        self.flip_a += (diff - self.flip_a) * 0.9;
        self.flip += self.flip_a;
    }

    pub fn rot_lerp(&self, pt: f32) -> f32 {
        self.o_rot + wrap_pi(self.rot - self.o_rot) * pt
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawner_spins_only_with_a_player_and_a_mob_and_counts_down_to_zero() {
        let mut a = Anim::default();
        a.event(BlockEntityKind::Spawner, 1, 0, 0);
        assert_eq!(a.delay, 200);
        a.tick_spawner(false);
        assert_eq!(a.spin, 0.0);
        assert_eq!(a.delay, 200);
        a.tick_spawner(true);
        assert_eq!(a.delay, 199);
        assert!((a.spin - 1000.0 / 399.0).abs() < 1e-4);
        for _ in 0..400 {
            a.tick_spawner(true);
        }
        assert_eq!(a.delay, 0);
    }

    #[test]
    fn trial_spawner_spins_by_state() {
        let mut a = Anim::default();
        a.tick_trial_spawner("cooldown", 0);
        assert_eq!(a.spin, 0.0);
        a.tick_trial_spawner("waiting_for_players", 0);
        assert!((a.spin - 1.0).abs() < 1e-5);
        a.tick_trial_spawner("active", -5);
        assert!((a.spin - 1.0 - 5.0).abs() < 1e-4);
    }

    #[test]
    fn chest_lid_opens_and_closes_in_ten_ticks() {
        let mut a = Anim::default();
        a.event(BlockEntityKind::Chest, 1, 1, 0);
        for _ in 0..10 {
            a.tick(BlockEntityKind::Chest, false, None);
        }
        assert!((a.lid(1.0) - 1.0).abs() < 1e-5);
        a.event(BlockEntityKind::Chest, 1, 0, 0);
        a.tick(BlockEntityKind::Chest, false, None);
        assert!((a.lid(0.0) - 1.0).abs() < 1e-5);
        assert!((a.lid(1.0) - 0.9).abs() < 1e-5);
    }

    #[test]
    fn bell_stops_after_fifty_ticks() {
        let mut a = Anim::default();
        a.event(BlockEntityKind::Bell, 1, 2, 0);
        assert!(a.bell_shaking);
        for _ in 0..BELL_TICKS {
            a.tick(BlockEntityKind::Bell, false, None);
        }
        assert!(!a.bell_shaking);
    }
}
