//! Elytra gliding, firework boosts and riptide spins.

use crate::aabb::Aabb;
use crate::player::PlayerPhysics;
use crate::trig::{self, DEG_TO_RAD};
use crate::world::CollisionWorld;

pub const RIPTIDE_SPIN_TICKS: i32 = 20;
const RIPTIDE_LIFT: f64 = 1.1999999f32 as f64;

impl PlayerPhysics {
    /// `Entity#getLookAngle` from the current rotation.
    pub fn look_angle(&self) -> [f64; 3] {
        let x = self.pitch * DEG_TO_RAD;
        let y = -self.yaw * DEG_TO_RAD;
        let (cos_y, sin_y) = (trig::cos(f64::from(y)), trig::sin(f64::from(y)));
        let (cos_x, sin_x) = (trig::cos(f64::from(x)), trig::sin(f64::from(x)));
        [
            f64::from(sin_y * cos_x),
            f64::from(-sin_x),
            f64::from(cos_y * cos_x),
        ]
    }

    /// `Player#canGlide`: never while creative flying.
    pub(crate) fn can_glide(&self) -> bool {
        !self.abilities.flying
            && !self.on_ground
            && !self.passenger
            && self.effects.levitation.is_none()
            && self.glider_equipped
    }

    pub(crate) fn is_in_liquid(&self) -> bool {
        self.is_in_water() || self.is_in_lava()
    }

    /// `Player#tryToStartFallFlying`; true when the start command must be sent.
    pub(crate) fn try_start_fall_flying(&mut self) -> bool {
        if !self.fall_flying && self.can_glide() && !self.is_in_liquid() {
            self.fall_flying = true;
            return true;
        }
        false
    }

    pub(crate) fn check_fall_distance_accumulation(&mut self) {
        if self.vel[1] > -0.5 && self.fall_distance > 1.0 {
            self.fall_distance = 1.0;
        }
    }

    pub(crate) fn travel_fall_flying<W: CollisionWorld + ?Sized>(
        &mut self,
        w: &W,
        input: [f64; 3],
    ) {
        if self.on_climbable(w) {
            self.travel_in_air(w, input);
            self.fall_flying = false;
            return;
        }
        self.vel = self.fall_flying_movement(self.vel);
        let v = self.vel;
        self.do_move(w, v);
    }

    pub(crate) fn fall_flying_movement(&self, mut m: [f64; 3]) -> [f64; 3] {
        let look = self.look_angle();
        let lean = self.pitch * DEG_TO_RAD;
        let look_hor = (look[0] * look[0] + look[2] * look[2]).sqrt();
        let move_hor = (m[0] * m[0] + m[2] * m[2]).sqrt();
        let gravity = self.effective_gravity();
        let cos = f64::from(lean).cos();
        let lift = cos * cos;
        m[1] += gravity * (-1.0 + lift * 0.75);
        if m[1] < 0.0 && look_hor > 0.0 {
            let convert = m[1] * -0.1 * lift;
            m[0] += look[0] * convert / look_hor;
            m[1] += convert;
            m[2] += look[2] * convert / look_hor;
        }
        if lean < 0.0 && look_hor > 0.0 {
            let convert = move_hor * f64::from(-trig::sin(f64::from(lean))) * 0.04;
            m[0] += -look[0] * convert / look_hor;
            m[1] += convert * 3.2;
            m[2] += -look[2] * convert / look_hor;
        }
        if look_hor > 0.0 {
            m[0] += (look[0] / look_hor * move_hor - m[0]) * 0.1;
            m[2] += (look[2] / look_hor * move_hor - m[2]) * 0.1;
        }
        [
            m[0] * f64::from(0.99f32),
            m[1] * f64::from(0.98f32),
            m[2] * f64::from(0.99f32),
        ]
    }

    /// An attached firework rocket pushing a gliding player along the view direction.
    pub fn apply_firework_boost(&mut self) {
        if !self.fall_flying {
            return;
        }
        let look = self.look_angle();
        for a in 0..3 {
            self.vel[a] += look[a] * 0.1 + (look[a] * 1.5 - self.vel[a]) * 0.5;
        }
    }

    /// Releasing a riptide trident: the push, the spin and the hop off the ground.
    pub fn start_riptide<W: CollisionWorld + ?Sized>(&mut self, w: &W, strength: f32) {
        let (yaw, pitch) = (self.yaw * DEG_TO_RAD, self.pitch * DEG_TO_RAD);
        let mut xd = -trig::sin(f64::from(yaw)) * trig::cos(f64::from(pitch));
        let mut yd = -trig::sin(f64::from(pitch));
        let mut zd = trig::cos(f64::from(yaw)) * trig::cos(f64::from(pitch));
        let dist = (xd * xd + yd * yd + zd * zd).sqrt();
        xd *= strength / dist;
        yd *= strength / dist;
        zd *= strength / dist;
        self.vel[0] += f64::from(xd);
        self.vel[1] += f64::from(yd);
        self.vel[2] += f64::from(zd);
        self.spin_attack_ticks = RIPTIDE_SPIN_TICKS;
        if self.on_ground {
            self.do_move(w, [0.0, RIPTIDE_LIFT, 0.0]);
        }
    }

    /// `LivingEntity#checkAutoSpinAttack`: the spin ends on a wall and bounces off the first living
    /// entity swept through.
    pub(crate) fn tick_spin_attack(&mut self, before: &Aabb) {
        if self.spin_attack_ticks <= 0 {
            return;
        }
        self.spin_attack_ticks -= 1;
        let now = self.bounding_box();
        let swept = Aabb::new(
            std::array::from_fn(|a| before.min[a].min(now.min[a])),
            std::array::from_fn(|a| before.max[a].max(now.max[a])),
        );
        let mut touched = false;
        for (bb, living) in &self.spin_targets {
            if !bb.intersects(&swept) {
                continue;
            }
            touched = true;
            if *living {
                self.spin_attack_ticks = 0;
                self.vel = self.vel.map(|v| v * -0.2);
                return;
            }
        }
        if !touched && self.horizontal_collision {
            self.spin_attack_ticks = 0;
        }
    }
}
