//! Client-side simulation of simple non-living entities: dropped items, XP orbs, TNT, falling
//! blocks and every projectile the vanilla client keeps moving between server updates.

use crate::aabb::Aabb;
use crate::collide::{Colliders, CollisionCtx, any_collision};
use crate::data::{FLUID_LAVA, FLUID_WATER, PhysicsData, StateInfo, flag};
use crate::world::CollisionWorld;

pub const MAX_SPEED: f64 = 10.0;
const MAX_COORD: f64 = 3.0e7;
const AIR_DRAG: f32 = 0.98;
const SAME_AXIS_EPS: f64 = 1.0e-5;
const ORB_FOLLOW_RANGE: f64 = 8.0;
const BELOW_OFFSET: f64 = 0.999999_f32 as f64;
const RESTING_SPEED_SQR: f64 = 1.0e-5_f32 as f64;
const FLUID_LIFT: f64 = 5.0e-4_f32 as f64;
const FLUID_LIFT_CAP: f64 = 0.06_f32 as f64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyKind {
    Item,
    ExperienceOrb,
    Tnt,
    FallingBlock,
    Thrown,
    Potion,
    ExperienceBottle,
    Arrow,
    Trident,
    /// Fireballs: accelerate along their motion, no gravity.
    Fireball,
    WitherSkull,
    /// Wind charges keep their speed: no acceleration and no drag.
    WindCharge,
    LlamaSpit,
    FireworkRocket,
    /// Eyes of ender and shulker bullets: straight lines the server steers.
    Drift,
}

impl BodyKind {
    fn gravity(self) -> f64 {
        match self {
            Self::Item | Self::Tnt | Self::FallingBlock => 0.04,
            Self::ExperienceOrb | Self::Thrown => 0.03,
            Self::Arrow | Self::Trident | Self::Potion => 0.05,
            Self::ExperienceBottle => 0.07,
            Self::LlamaSpit => 0.06,
            Self::Fireball
            | Self::WitherSkull
            | Self::WindCharge
            | Self::FireworkRocket
            | Self::Drift => 0.0,
        }
    }

    pub fn is_projectile(self) -> bool {
        !matches!(
            self,
            Self::Item | Self::ExperienceOrb | Self::Tnt | Self::FallingBlock
        )
    }
}

#[derive(Debug, Default)]
pub struct BodyScratch(Colliders);

/// Synced state that steers a projectile: entity data flags and the entity it follows.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BodyFlags {
    pub no_gravity: bool,
    /// `AbstractArrow#isNoPhysics`: passes through blocks, as a returning trident does.
    pub no_physics: bool,
    /// A blue wither skull, which slows more.
    pub dangerous: bool,
    /// A crossbow rocket flies straight instead of accelerating upwards.
    pub shot_at_angle: bool,
    pub loyalty: u8,
    /// `AbstractArrow#isInGround` as the server synced it.
    pub in_ground: bool,
    /// `ThrownTrident#dealtDamage`: set after more than four ticks in the ground.
    pub dealt_damage: bool,
    /// The owner's eye a loyal trident returns to, or the entity an attached rocket rides with.
    pub anchor: Option<[f64; 3]>,
    /// A hurting projectile's `accelerationPower` from `projectile_power`.
    pub acceleration: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Body {
    pub kind: BodyKind,
    pub width: f64,
    pub height: f64,
    pub id: i32,
    pub tick_count: u32,
    pub pos: [f64; 3],
    pub vel: [f64; 3],
    pub on_ground: bool,
    pub horizontal_collision: bool,
    /// `AbstractArrow#inGroundTime`.
    pub in_ground_time: u32,
    pub flags: BodyFlags,
}

#[derive(Clone, Copy)]
struct Hit {
    below: bool,
}

impl Body {
    pub fn new(kind: BodyKind, id: i32, size: (f64, f64), pos: [f64; 3], vel: [f64; 3]) -> Self {
        Self {
            kind,
            width: size.0,
            height: size.1,
            id,
            tick_count: 0,
            pos,
            vel,
            on_ground: false,
            horizontal_collision: false,
            in_ground_time: 0,
            flags: BodyFlags::default(),
        }
    }

    pub fn bounding_box(&self) -> Aabb {
        let h = self.width / 2.0;
        Aabb::new(
            [self.pos[0] - h, self.pos[1], self.pos[2] - h],
            [self.pos[0] + h, self.pos[1] + self.height, self.pos[2] + h],
        )
    }

    /// `follow` is the point an XP orb homes toward: the nearest player's mid-eye.
    pub fn tick<W: CollisionWorld + ?Sized>(
        &mut self,
        world: &W,
        data: &PhysicsData,
        scratch: &mut BodyScratch,
        follow: Option<[f64; 3]>,
    ) {
        self.tick_count = self.tick_count.wrapping_add(1);
        if !self.sanitize() {
            return;
        }
        match self.kind {
            BodyKind::Item => self.tick_item(world, data, scratch),
            BodyKind::ExperienceOrb => self.tick_orb(world, data, scratch, follow),
            BodyKind::Tnt | BodyKind::FallingBlock => self.tick_block(world, data, scratch),
            BodyKind::Thrown | BodyKind::Potion | BodyKind::ExperienceBottle => {
                self.tick_thrown(world, data, scratch)
            }
            BodyKind::Arrow | BodyKind::Trident => self.tick_arrow(world, data, scratch),
            BodyKind::Fireball | BodyKind::WitherSkull | BodyKind::WindCharge => {
                self.tick_hurting(world, data, scratch)
            }
            BodyKind::LlamaSpit => self.tick_spit(world, data),
            BodyKind::FireworkRocket => self.tick_firework(world, data, scratch),
            BodyKind::Drift => {
                for i in 0..3 {
                    self.pos[i] += self.vel[i];
                }
            }
        }
    }

    fn gravity(&self) -> f64 {
        if self.flags.no_gravity {
            0.0
        } else {
            self.kind.gravity()
        }
    }

    /// Server data is hostile: refuse non-finite or absurd state and bound the swept box.
    fn sanitize(&mut self) -> bool {
        if !self
            .pos
            .iter()
            .all(|v| v.is_finite() && v.abs() <= MAX_COORD)
        {
            self.vel = [0.0; 3];
            return false;
        }
        for v in &mut self.vel {
            *v = if v.is_finite() {
                v.clamp(-MAX_SPEED, MAX_SPEED)
            } else {
                0.0
            };
        }
        true
    }

    fn tick_item<W: CollisionWorld + ?Sized>(
        &mut self,
        world: &W,
        data: &PhysicsData,
        scratch: &mut BodyScratch,
    ) {
        let bb = self.bounding_box();
        if fluid_height(world, data, &bb, FLUID_WATER) > 0.1 {
            self.float_in_fluid(0.99);
        } else if fluid_height(world, data, &bb, FLUID_LAVA) > 0.1 {
            self.float_in_fluid(0.95);
        } else {
            self.vel[1] -= self.kind.gravity();
        }
        let resting = self.on_ground
            && self.horizontal_speed_sqr() <= RESTING_SPEED_SQR
            && (i64::from(self.tick_count) + i64::from(self.id)) % 4 != 0;
        if resting {
            return;
        }
        self.step(world, data, scratch);
        let friction = f64::from(self.ground_friction(world, data));
        self.vel[0] *= friction;
        self.vel[1] *= f64::from(AIR_DRAG);
        self.vel[2] *= friction;
        if self.on_ground && self.vel[1] < 0.0 {
            self.vel[1] *= -0.5;
        }
    }

    fn tick_orb<W: CollisionWorld + ?Sized>(
        &mut self,
        world: &W,
        data: &PhysicsData,
        scratch: &mut BodyScratch,
        follow: Option<[f64; 3]>,
    ) {
        if self.tick_count == 1 {
            return;
        }
        let colliding = any_collision(world, data, &self.bounding_box(), &CollisionCtx::default());
        let eye = [self.pos[0], self.pos[1] + self.height * 0.85, self.pos[2]];
        if fluid_at(world, data, eye, FLUID_WATER) {
            self.vel[0] *= 0.99;
            self.vel[1] = (self.vel[1] + FLUID_LIFT).min(FLUID_LIFT_CAP);
            self.vel[2] *= 0.99;
        } else if !colliding {
            self.vel[1] -= self.kind.gravity();
        }
        if let Some(target) = follow {
            let d = [
                target[0] - self.pos[0],
                target[1] - self.pos[1],
                target[2] - self.pos[2],
            ];
            let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            if len > 0.0 && len <= ORB_FOLLOW_RANGE {
                let power = 1.0 - len / ORB_FOLLOW_RANGE;
                let k = power * power * 0.1 / len;
                for i in 0..3 {
                    self.vel[i] += d[i] * k;
                }
            }
        }
        let fall_speed = self.vel[1];
        let hit = self.step(world, data, scratch);
        let friction = f64::from(self.ground_friction(world, data));
        for v in &mut self.vel {
            *v *= friction;
        }
        if hit.below && fall_speed < -self.kind.gravity() {
            self.vel[1] = -fall_speed * 0.4;
        }
    }

    fn tick_block<W: CollisionWorld + ?Sized>(
        &mut self,
        world: &W,
        data: &PhysicsData,
        scratch: &mut BodyScratch,
    ) {
        self.vel[1] -= self.kind.gravity();
        self.step(world, data, scratch);
        for v in &mut self.vel {
            *v *= f64::from(AIR_DRAG);
        }
        if self.kind == BodyKind::Tnt && self.on_ground {
            self.vel = [self.vel[0] * 0.7, self.vel[1] * -0.5, self.vel[2] * 0.7];
        }
    }

    fn in_water<W: CollisionWorld + ?Sized>(&self, world: &W, data: &PhysicsData) -> bool {
        fluid_height(world, data, &self.bounding_box(), FLUID_WATER) > 0.0
    }

    fn scale_vel(&mut self, k: f64) {
        for v in &mut self.vel {
            *v *= k;
        }
    }

    /// Moves by the velocity and stops dead on contact; the server removes the projectile when it
    /// actually hits something. Returns false once stopped.
    fn fly<W: CollisionWorld + ?Sized>(
        &mut self,
        world: &W,
        data: &PhysicsData,
        scratch: &mut BodyScratch,
    ) -> bool {
        let movement = self.vel;
        if movement == [0.0; 3] {
            return false;
        }
        let resolved = collide(world, data, scratch, &self.bounding_box(), movement);
        for i in 0..3 {
            self.pos[i] += resolved[i];
        }
        if resolved != movement {
            self.vel = [0.0; 3];
            return false;
        }
        true
    }

    /// `ThrowableProjectile#tick`: gravity and drag come before the move.
    fn tick_thrown<W: CollisionWorld + ?Sized>(
        &mut self,
        world: &W,
        data: &PhysicsData,
        scratch: &mut BodyScratch,
    ) {
        if self.vel == [0.0; 3] {
            return;
        }
        self.vel[1] -= self.gravity();
        let drag = if self.in_water(world, data) {
            0.8
        } else {
            0.99
        };
        self.scale_vel(f64::from(drag as f32));
        self.fly(world, data, scratch);
    }

    /// `ThrownTrident#tick` then `AbstractArrow#tick`: a loyal trident that dealt damage or lost its
    /// physics homes in on its owner's eye, and one stuck for more than four ticks counts as having
    /// dealt damage.
    fn tick_arrow<W: CollisionWorld + ?Sized>(
        &mut self,
        world: &W,
        data: &PhysicsData,
        scratch: &mut BodyScratch,
    ) {
        if self.kind == BodyKind::Trident {
            if self.in_ground_time > 4 {
                self.flags.dealt_damage = true;
            }
            if self.flags.loyalty > 0
                && (self.flags.dealt_damage || self.flags.no_physics)
                && let Some(eye) = self.flags.anchor
            {
                self.flags.no_physics = true;
                let to = [
                    eye[0] - self.pos[0],
                    eye[1] - self.pos[1],
                    eye[2] - self.pos[2],
                ];
                let loyalty = f64::from(self.flags.loyalty);
                self.pos[1] += to[1] * 0.015 * loyalty;
                let dir = normalize(to);
                for i in 0..3 {
                    self.vel[i] = self.vel[i] * 0.95 + dir[i] * 0.05 * loyalty;
                }
            }
        }
        let physics = !self.flags.no_physics;
        if physics && (self.flags.in_ground || self.vel == [0.0; 3]) {
            self.in_ground_time = self.in_ground_time.saturating_add(1);
            return;
        }
        self.in_ground_time = 0;
        let in_water = self.in_water(world, data);
        if in_water {
            let inertia = if self.kind == BodyKind::Trident {
                0.99f32
            } else {
                0.6
            };
            self.scale_vel(f64::from(inertia));
        }
        if physics {
            if !self.fly(world, data, scratch) {
                return;
            }
        } else {
            for i in 0..3 {
                self.pos[i] += self.vel[i];
            }
        }
        if !in_water {
            self.scale_vel(f64::from(0.99f32));
        }
        if physics {
            self.vel[1] -= self.gravity();
        }
    }

    /// `AbstractHurtingProjectile#tick`: speeds up along its motion, then moves.
    fn tick_hurting<W: CollisionWorld + ?Sized>(
        &mut self,
        world: &W,
        data: &PhysicsData,
        scratch: &mut BodyScratch,
    ) {
        let (default_accel, inertia, liquid) = match self.kind {
            BodyKind::WindCharge => (0.0, 1.0, 1.0),
            BodyKind::WitherSkull if self.flags.dangerous => (0.1, 0.73f32, 0.8f32),
            _ => (0.1, 0.95, 0.8),
        };
        let accel = self
            .flags
            .acceleration
            .filter(|a| a.is_finite())
            .map_or(default_accel, |a| a.clamp(-MAX_SPEED, MAX_SPEED));
        let k = if self.in_water(world, data) {
            liquid
        } else {
            inertia
        };
        let dir = normalize(self.vel);
        for i in 0..3 {
            self.vel[i] = (self.vel[i] + dir[i] * accel) * f64::from(k);
        }
        self.fly(world, data, scratch);
    }

    /// `LlamaSpit#tick`: moves by the old motion, which then slows and falls.
    fn tick_spit<W: CollisionWorld + ?Sized>(&mut self, world: &W, data: &PhysicsData) {
        if self.vel == [0.0; 3] {
            return;
        }
        let bb = self.bounding_box();
        let lo = bb.min.map(|v| v.floor() as i32);
        let hi = bb.max.map(|v| v.floor() as i32);
        let mut any_air = false;
        for x in lo[0]..=hi[0] {
            for y in lo[1]..=hi[1] {
                for z in lo[2]..=hi[2] {
                    any_air |= state_at(world, data, [x, y, z]).is_air();
                }
            }
        }
        if !any_air || self.in_water(world, data) {
            self.vel = [0.0; 3];
            return;
        }
        let movement = self.vel;
        self.scale_vel(f64::from(0.99f32));
        self.vel[1] -= self.gravity();
        for i in 0..3 {
            self.pos[i] += movement[i];
        }
    }

    /// `FireworkRocketEntity#tick`: a free rocket climbs and speeds up sideways; an attached one
    /// rides with its entity.
    fn tick_firework<W: CollisionWorld + ?Sized>(
        &mut self,
        world: &W,
        data: &PhysicsData,
        scratch: &mut BodyScratch,
    ) {
        if let Some(at) = self.flags.anchor {
            self.pos = at;
            return;
        }
        if !self.flags.shot_at_angle {
            let k = if self.horizontal_collision { 1.0 } else { 1.15 };
            self.vel = [self.vel[0] * k, self.vel[1] + 0.04, self.vel[2] * k];
        }
        let movement = self.vel;
        let moved = collide(world, data, scratch, &self.bounding_box(), movement);
        for i in 0..3 {
            self.pos[i] += moved[i];
        }
        self.horizontal_collision = (movement[0] - moved[0]).abs() >= SAME_AXIS_EPS
            || (movement[2] - moved[2]).abs() >= SAME_AXIS_EPS;
    }

    fn float_in_fluid(&mut self, multiplier: f64) {
        self.vel[0] *= multiplier;
        if self.vel[1] < FLUID_LIFT_CAP {
            self.vel[1] += FLUID_LIFT;
        }
        self.vel[2] *= multiplier;
    }

    fn horizontal_speed_sqr(&self) -> f64 {
        self.vel[0] * self.vel[0] + self.vel[2] * self.vel[2]
    }

    fn block_below<W: CollisionWorld + ?Sized>(&self, world: &W, data: &PhysicsData) -> StateInfo {
        let p = block_pos(self.pos[0], self.pos[1] - BELOW_OFFSET, self.pos[2]);
        *state_at(world, data, p)
    }

    fn ground_friction<W: CollisionWorld + ?Sized>(&self, world: &W, data: &PhysicsData) -> f32 {
        if self.on_ground {
            AIR_DRAG * self.block_below(world, data).friction
        } else {
            AIR_DRAG
        }
    }

    /// Vanilla `Entity.move` for a body without step-up: collide, then zero or bounce the blocked axes.
    fn step<W: CollisionWorld + ?Sized>(
        &mut self,
        world: &W,
        data: &PhysicsData,
        scratch: &mut BodyScratch,
    ) -> Hit {
        let delta = self.vel;
        let moved = collide(world, data, scratch, &self.bounding_box(), delta);
        let len_sqr = |v: [f64; 3]| v[0] * v[0] + v[1] * v[1] + v[2] * v[2];
        if len_sqr(moved) > 1.0e-7 || len_sqr(delta) - len_sqr(moved) < 1.0e-7 {
            for i in 0..3 {
                self.pos[i] += moved[i];
            }
        }
        let blocked = |i: usize| (delta[i] - moved[i]).abs() >= SAME_AXIS_EPS;
        let vertical = delta[1] != moved[1];
        let below = vertical && delta[1] < 0.0;
        if delta[1] != 0.0 {
            self.on_ground = below;
        }
        if (delta[1] != 0.0 && vertical) || blocked(0) || blocked(2) {
            let restitution = if below && -delta[1] > self.kind.gravity() {
                f64::from(self.block_below(world, data).bounce * 0.8)
            } else {
                0.0
            };
            self.restitute([blocked(0), blocked(2)], vertical, restitution, moved[1]);
        }
        let here = *state_at(
            world,
            data,
            block_pos(self.pos[0], self.pos[1], self.pos[2]),
        );
        let factor = if here.has(flag::WATER_BLOCK) || here.speed_factor != 1.0 {
            here.speed_factor
        } else {
            self.block_below(world, data).speed_factor
        };
        self.vel[0] *= f64::from(factor);
        self.vel[2] *= f64::from(factor);
        Hit { below }
    }

    fn restitute(&mut self, blocked: [bool; 2], vertical: bool, restitution: f64, moved_y: f64) {
        let current = self.vel;
        if blocked[0] {
            self.vel[0] = 0.0;
        }
        if blocked[1] {
            self.vel[2] = 0.0;
        }
        if vertical {
            self.vel[1] = if restitution > 0.0 {
                let portion = moved_y / current[1];
                let compensation = portion * self.kind.gravity();
                let drag = 1.0 + (f64::from(AIR_DRAG) - 1.0) * portion;
                (compensation - current[1]) * drag * restitution
            } else {
                0.0
            };
        }
    }
}

fn collide<W: CollisionWorld + ?Sized>(
    world: &W,
    data: &PhysicsData,
    scratch: &mut BodyScratch,
    bb: &Aabb,
    movement: [f64; 3],
) -> [f64; 3] {
    if movement == [0.0; 3] {
        return movement;
    }
    scratch.0.gather(
        world,
        data,
        &bb.expand_towards(movement),
        &CollisionCtx::default(),
    );
    scratch.0.collide_with_shapes(movement, bb)
}

fn block_pos(x: f64, y: f64, z: f64) -> [i32; 3] {
    [x.floor() as i32, y.floor() as i32, z.floor() as i32]
}

fn state_at<'a, W: CollisionWorld + ?Sized>(
    world: &W,
    data: &'a PhysicsData,
    p: [i32; 3],
) -> &'a StateInfo {
    data.state(world.block_state(p[0], p[1], p[2]))
}

fn surface<W: CollisionWorld + ?Sized>(
    world: &W,
    data: &PhysicsData,
    p: [i32; 3],
    s: &StateInfo,
) -> f64 {
    if state_at(world, data, [p[0], p[1] + 1, p[2]]).fluid == s.fluid {
        1.0
    } else {
        f64::from(s.own_fluid_height())
    }
}

fn fluid_at<W: CollisionWorld + ?Sized>(
    world: &W,
    data: &PhysicsData,
    at: [f64; 3],
    fluid: u8,
) -> bool {
    let p = block_pos(at[0], at[1], at[2]);
    let s = state_at(world, data, p);
    s.fluid == fluid && f64::from(p[1]) + surface(world, data, p, s) > at[1]
}

/// `Entity#isEyeInFluid(WATER)` for an eye point already lowered by vanilla's 1/9 block.
pub fn eye_in_water<W: CollisionWorld + ?Sized>(
    world: &W,
    data: &PhysicsData,
    at: [f64; 3],
) -> bool {
    at.iter().all(|v| v.is_finite()) && fluid_at(world, data, at, FLUID_WATER)
}

/// Depth of the deepest `fluid` overlapping `bb`, measured from the box bottom (vanilla `getFluidHeight`).
pub fn fluid_height<W: CollisionWorld + ?Sized>(
    world: &W,
    data: &PhysicsData,
    bb: &Aabb,
    fluid: u8,
) -> f64 {
    let bb = bb.inflate(-0.001);
    let mut deepest = 0.0f64;
    for y in bb.min[1].floor() as i32..bb.max[1].ceil() as i32 {
        for z in bb.min[2].floor() as i32..bb.max[2].ceil() as i32 {
            for x in bb.min[0].floor() as i32..bb.max[0].ceil() as i32 {
                let p = [x, y, z];
                let s = state_at(world, data, p);
                if s.fluid != fluid {
                    continue;
                }
                let top = f64::from(y) + surface(world, data, p, s);
                if top >= bb.min[1] {
                    deepest = deepest.max(top - bb.min[1]);
                }
            }
        }
    }
    deepest
}

/// `Vec3#normalize`: zero below a length of 1e-5.
fn normalize(v: [f64; 3]) -> [f64; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len < 1.0e-5 {
        [0.0; 3]
    } else {
        [v[0] / len, v[1] / len, v[2] / len]
    }
}

/// Yaw and pitch in degrees that point along `vel`, as projectiles orient themselves.
pub fn aim(vel: [f64; 3]) -> (f32, f32) {
    let horizontal = vel[0].hypot(vel[2]);
    (
        vel[0].atan2(vel[2]).to_degrees() as f32,
        vel[1].atan2(horizontal).to_degrees() as f32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::TestWorld;

    fn item(pos: [f64; 3], vel: [f64; 3]) -> Body {
        Body::new(BodyKind::Item, 7, (0.25, 0.25), pos, vel)
    }

    fn tick(b: &mut Body, w: &TestWorld, n: usize) {
        let data = PhysicsData::v26_3();
        let mut scratch = BodyScratch::default();
        for _ in 0..n {
            b.tick(w, data, &mut scratch, None);
        }
    }

    #[test]
    fn falling_item_matches_hand_computed_ticks() {
        let w = TestWorld::flat("minecraft:stone", 64);
        let mut b = item([0.5, 70.0, 0.5], [0.0; 3]);
        tick(&mut b, &w, 1);
        assert!((b.pos[1] - 69.96).abs() < 1e-9);
        let drag = f64::from(0.98f32);
        assert!((b.vel[1] - (-0.04 * drag)).abs() < 1e-12);
        tick(&mut b, &w, 1);
        let vy2 = -0.04 * drag - 0.04;
        assert!((b.pos[1] - (69.96 + vy2)).abs() < 1e-9);
        assert!((b.vel[1] - vy2 * drag).abs() < 1e-12);
    }

    #[test]
    fn item_lands_and_rests_on_the_floor() {
        let w = TestWorld::flat("minecraft:stone", 64);
        let mut b = item([0.5, 66.0, 0.5], [0.0; 3]);
        tick(&mut b, &w, 60);
        assert!(b.on_ground);
        assert_eq!(b.pos[1], 64.0);
    }

    #[test]
    fn sliding_item_loses_speed_by_block_friction() {
        let w = TestWorld::flat("minecraft:stone", 64);
        let mut b = item([0.5, 64.0, 0.5], [0.1, 0.0, 0.0]);
        b.on_ground = true;
        tick(&mut b, &w, 1);
        let friction = f64::from(0.98f32 * 0.6f32);
        assert!((b.pos[0] - 0.6).abs() < 1e-9);
        assert!((b.vel[0] - 0.1 * friction).abs() < 1e-9);
        tick(&mut b, &w, 1);
        assert!((b.pos[0] - (0.6 + 0.1 * friction)).abs() < 1e-9);
        assert!((b.vel[0] - 0.1 * friction * friction).abs() < 1e-9);
    }

    #[test]
    fn item_stops_against_a_wall() {
        let mut w = TestWorld::flat("minecraft:stone", 64);
        w.fill(
            [1, 64, -5],
            [1, 66, 5],
            crate::tests::default_state("minecraft:stone"),
        );
        let mut b = item([0.5, 64.0, 0.5], [0.3, 0.0, 0.0]);
        b.on_ground = true;
        tick(&mut b, &w, 3);
        assert!((b.pos[0] - (1.0 - 0.125)).abs() < 1e-6);
        assert_eq!(b.vel[0], 0.0);
    }

    #[test]
    fn dropped_item_flies_in_an_arc_before_landing() {
        let w = TestWorld::flat("minecraft:stone", 64);
        let mut b = item([0.5, 65.3, 0.5], [0.0, 0.2, 0.3]);
        tick(&mut b, &w, 5);
        assert!(b.pos[2] > 1.2 && b.pos[1] > 65.3);
        tick(&mut b, &w, 80);
        assert!(b.on_ground && b.pos[1] == 64.0);
        assert!(b.pos[2] > 1.5);
    }

    #[test]
    fn hostile_state_is_contained() {
        let w = TestWorld::flat("minecraft:stone", 64);
        let nan = f64::NAN;
        for (pos, vel) in [
            ([nan, 70.0, 0.0], [0.0; 3]),
            ([0.0, f64::INFINITY, 0.0], [0.0; 3]),
            ([1e300, 70.0, 0.0], [0.0; 3]),
            ([0.5, 70.0, 0.5], [nan, f64::INFINITY, 1e300]),
            ([0.5, 70.0, 0.5], [f64::NEG_INFINITY, -1e300, nan]),
        ] {
            for kind in [
                BodyKind::Item,
                BodyKind::ExperienceOrb,
                BodyKind::Arrow,
                BodyKind::Tnt,
            ] {
                let mut b = Body::new(kind, 1, (0.25, 0.25), pos, vel);
                tick(&mut b, &w, 3);
                assert!(b.vel.iter().all(|v| v.is_finite() && v.abs() <= MAX_SPEED));
            }
        }
    }

    #[test]
    fn arrow_stops_in_a_block() {
        let mut w = TestWorld::flat("minecraft:stone", 64);
        w.fill(
            [3, 60, -5],
            [3, 80, 5],
            crate::tests::default_state("minecraft:stone"),
        );
        let mut b = Body::new(
            BodyKind::Arrow,
            1,
            (0.5, 0.5),
            [0.0, 70.0, 0.0],
            [1.0, 0.0, 0.0],
        );
        tick(&mut b, &w, 8);
        assert_eq!(b.vel, [0.0; 3]);
        assert!(b.pos[0] < 3.0);
    }

    #[test]
    fn loyal_trident_returns_after_five_ground_ticks() {
        let w = TestWorld::flat("minecraft:stone", 64);
        let mut b = Body::new(BodyKind::Trident, 1, (0.5, 0.5), [0.5, 65.0, 0.5], [0.0; 3]);
        b.flags.loyalty = 3;
        b.flags.in_ground = true;
        b.flags.anchor = Some([0.5, 70.0, 6.5]);
        for t in 1..=5 {
            tick(&mut b, &w, 1);
            assert_eq!(b.in_ground_time, t);
            assert_eq!(b.pos, [0.5, 65.0, 0.5], "still stuck at tick {t}");
            assert!(!b.flags.dealt_damage && !b.flags.no_physics);
        }
        tick(&mut b, &w, 1);
        assert!(
            b.flags.dealt_damage && b.flags.no_physics,
            "returns on the sixth tick"
        );
        assert_eq!(b.in_ground_time, 0);
        assert!(b.pos[1] > 65.0 && b.pos[2] > 0.5, "moves toward the owner");

        let mut plain = Body::new(BodyKind::Trident, 2, (0.5, 0.5), [0.5, 65.0, 0.5], [0.0; 3]);
        plain.flags.in_ground = true;
        plain.flags.anchor = Some([0.5, 70.0, 6.5]);
        tick(&mut plain, &w, 10);
        assert_eq!(plain.pos, [0.5, 65.0, 0.5], "no loyalty, no return");

        let mut flying = Body::new(
            BodyKind::Trident,
            3,
            (0.5, 0.5),
            [0.5, 70.0, 0.5],
            [0.0, 0.0, 1.0],
        );
        flying.flags.loyalty = 3;
        flying.flags.anchor = Some([0.5, 70.0, -6.5]);
        tick(&mut flying, &w, 3);
        assert!(flying.pos[2] > 2.5, "a loyal trident in flight keeps going");
    }
}
