//! Compass and clock needles: vanilla's `NeedleDirectionHelper` wobblers, stepped once per tick,
//! and the world inputs every item model that reads them gets.

use mcr_assets::CompassInputs;
use mcr_client::ClientState;
use mcr_entity_model::{ItemOwner, ItemWorld};

/// `NeedleDirectionHelper.standardWobbler`.
#[derive(Debug, Clone, Copy)]
struct Wobbler {
    rotation: f32,
    delta: f32,
    factor: f32,
}

impl Wobbler {
    const fn new(factor: f32) -> Self {
        Self {
            rotation: 0.0,
            delta: 0.0,
            factor,
        }
    }

    fn update(&mut self, target: f32) {
        if !target.is_finite() {
            return;
        }
        let step = (target - self.rotation + 0.5).rem_euclid(1.0) - 0.5;
        self.delta = (self.delta + step * 0.1) * self.factor;
        self.rotation = (self.rotation + self.delta).rem_euclid(1.0);
    }
}

#[derive(Debug, Clone)]
pub struct Needles {
    clock: Wobbler,
    clock_spin: Wobbler,
    compass: Wobbler,
    compass_spin: Wobbler,
    rng: u64,
}

impl Default for Needles {
    fn default() -> Self {
        Self {
            clock: Wobbler::new(0.9),
            clock_spin: Wobbler::new(0.9),
            compass: Wobbler::new(0.8),
            compass_spin: Wobbler::new(0.8),
            rng: 0x2545_F491_4F6C_DD1D,
        }
    }
}

impl Needles {
    fn next_float(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 40) as f32 / (1u64 << 24) as f32
    }

    /// One game tick: `sun` is the sun angle in turns, `body_yaw` the local player's in degrees.
    pub fn tick(&mut self, sun: f32, body_yaw: f32, runs_normally: bool) {
        self.clock.update(sun);
        let spin = self.next_float();
        self.clock_spin.update(spin);
        let spin = self.next_float();
        self.compass_spin.update(spin);
        if runs_normally {
            self.compass
                .update(0.5 - ((body_yaw / 360.0).rem_euclid(1.0) - 0.25));
        }
    }

    /// The inputs item models read this frame; `moon` is the moon phase in eighths.
    pub fn world(&self, state: &ClientState, moon: u8) -> ItemWorld {
        let dimension = state.dimension.name.as_str();
        let here = |dim: &str, pos: [i32; 3]| (dim == dimension).then_some(pos);
        let spawn = state
            .world
            .spawn
            .as_ref()
            .and_then(|s| here(s.dimension.as_str(), [s.pos.x, s.pos.y, s.pos.z]));
        let recovery = state
            .player
            .last_death
            .as_ref()
            .and_then(|d| here(d.dimension.as_str(), [d.pos.x, d.pos.y, d.pos.z]));
        ItemWorld {
            day_fraction: self.clock.rotation,
            random: self.clock_spin.rotation,
            moon_phase_fraction: f32::from(moon % 8) / 8.0,
            dimension: dimension.to_owned(),
            compass: CompassInputs {
                spawn,
                recovery,
                spin: self.compass_spin.rotation,
                ..CompassInputs::default()
            },
            local_wobble: state.world.runs_normally.then_some(self.compass.rotation),
            local_id: Some(state.player.entity_id),
        }
    }
}

/// The local player as the owner of first-person and inventory items.
pub fn local_owner(state: &ClientState, body_yaw: f32) -> ItemOwner {
    ItemOwner {
        id: state.player.entity_id,
        pos: state.player.pos(),
        yaw: body_yaw,
    }
}

/// Fills the GUI item context the way vanilla renders inventory items: owned by the local player.
pub fn apply_to_icons(ctx: &mut mcr_assets::ItemContext, world: &ItemWorld, owner: ItemOwner) {
    ctx.day_fraction = world.day_fraction;
    ctx.random = world.random;
    ctx.moon_phase_fraction = world.moon_phase_fraction;
    ctx.dimension.clone_from(&world.dimension);
    ctx.compass = CompassInputs {
        owner: Some((owner.pos, owner.yaw)),
        wobble: world.local_wobble,
        seed: owner.id,
        ..world.compass
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wobbler_settles_on_the_target_across_the_seam() {
        let mut w = Wobbler::new(0.8);
        for _ in 0..200 {
            w.update(0.95);
        }
        assert!((w.rotation - 0.95).abs() < 1e-3, "{}", w.rotation);
        for _ in 0..200 {
            w.update(0.05);
        }
        assert!((w.rotation - 0.05).abs() < 1e-3, "{}", w.rotation);
        w.update(f32::NAN);
        assert!(w.rotation.is_finite());
    }

    #[test]
    fn compass_points_toward_spawn() {
        let mut c = CompassInputs {
            owner: Some(([0.5, 64.0, 0.5], 0.0)),
            spawn: Some([0, 64, 100]),
            ..CompassInputs::default()
        };
        let toward = |c: &CompassInputs| c.angle(c.spawn);
        // Facing south with spawn straight ahead: the needle points up, turn 0.
        assert!(
            toward(&c).abs() < 1e-4 || (toward(&c) - 1.0).abs() < 1e-4,
            "{}",
            toward(&c)
        );
        // Turned to face west, spawn is on the left.
        c.owner = Some(([0.5, 64.0, 0.5], 90.0));
        assert!((toward(&c) - 0.75).abs() < 1e-4, "{}", toward(&c));
        // Spawn behind.
        c.owner = Some(([0.5, 64.0, 0.5], 180.0));
        assert!((toward(&c) - 0.5).abs() < 1e-4, "{}", toward(&c));
        // No owner reads 0, standing on the target spins.
        c.owner = None;
        assert_eq!(toward(&c), 0.0);
        c.owner = Some(([0.5, 64.5, 100.5], 0.0));
        c.spin = 0.3;
        assert!((toward(&c) - 0.3).abs() < 1e-4);
    }

    #[test]
    fn local_wobble_replaces_the_owner_yaw() {
        let c = CompassInputs {
            owner: Some(([0.5, 64.0, 0.5], 123.0)),
            spawn: Some([100, 64, 0]),
            wobble: Some(0.25),
            ..CompassInputs::default()
        };
        assert!((c.angle(c.spawn) - 0.25).abs() < 1e-4);
    }
}
