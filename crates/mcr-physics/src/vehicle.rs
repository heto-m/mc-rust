//! Client-authoritative simulation of vehicles the local player controls: boats and ridden land mobs.

use crate::aabb::Aabb;
use crate::data::{FLUID_WATER, flag};
use crate::player::PlayerPhysics;
use crate::trig;
use crate::types::{Attributes, PlayerInput};
use crate::world::CollisionWorld;

const DEG_TO_RAD: f32 = (std::f64::consts::PI / 180.0) as f32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoatStatus {
    InWater,
    UnderWater,
    UnderFlowingWater,
    OnLand,
    InAir,
}

fn floor(v: f64) -> i32 {
    v.floor() as i32
}

fn ceil(v: f64) -> i32 {
    v.ceil() as i32
}

#[derive(Debug, Clone)]
pub struct Boat {
    pub body: PlayerPhysics,
    status: BoatStatus,
    old_status: BoatStatus,
    water_level: f64,
    land_friction: f32,
    last_yd: f64,
    delta_rotation: f32,
    pub paddle: [bool; 2],
}

impl Boat {
    pub fn new(pos: [f64; 3], yaw: f32, width: f32, height: f32) -> Self {
        let mut body = PlayerPhysics::new(pos);
        body.dims = Some((width, height));
        body.yaw = yaw;
        body.attributes = Attributes {
            gravity: 0.04,
            step_height: 0.0,
            ..Attributes::default()
        };
        Self {
            body,
            status: BoatStatus::InAir,
            old_status: BoatStatus::InAir,
            water_level: 0.0,
            land_friction: 0.0,
            last_yd: 0.0,
            delta_rotation: 0.0,
            paddle: [false; 2],
        }
    }

    pub fn yaw(&self) -> f32 {
        self.body.yaw
    }

    /// Rotation to add to a rider's view this tick.
    pub fn delta_rotation(&self) -> f32 {
        self.delta_rotation
    }

    pub fn snap_to(&mut self, pos: [f64; 3], yaw: f32) {
        self.body.pos = pos;
        self.body.yaw = yaw;
    }

    fn state_fluid_height<W: CollisionWorld + ?Sized>(
        &self,
        w: &W,
        p: [i32; 3],
    ) -> Option<(f32, bool)> {
        let s = self.body.state_at(w, p);
        (s.fluid == FLUID_WATER).then(|| {
            (
                self.body.fluid_height_at(w, p, s),
                s.has(flag::FLUID_SOURCE),
            )
        })
    }

    fn under_water<W: CollisionWorld + ?Sized>(&self, w: &W) -> Option<BoatStatus> {
        let bb = self.body.bounding_box();
        let max_y = bb.max[1] + 0.001;
        let mut under = false;
        for x in floor(bb.min[0])..ceil(bb.max[0]) {
            for y in floor(bb.max[1])..ceil(max_y) {
                for z in floor(bb.min[2])..ceil(bb.max[2]) {
                    if let Some((h, source)) = self.state_fluid_height(w, [x, y, z])
                        && max_y < f64::from(y) + f64::from(h)
                    {
                        if !source {
                            return Some(BoatStatus::UnderFlowingWater);
                        }
                        under = true;
                    }
                }
            }
        }
        under.then_some(BoatStatus::UnderWater)
    }

    fn check_in_water<W: CollisionWorld + ?Sized>(&mut self, w: &W) -> bool {
        let bb = self.body.bounding_box();
        let mut in_water = false;
        self.water_level = -f64::MAX;
        for x in floor(bb.min[0])..ceil(bb.max[0]) {
            for y in floor(bb.min[1])..ceil(bb.min[1] + 0.001) {
                for z in floor(bb.min[2])..ceil(bb.max[2]) {
                    if let Some((h, _)) = self.state_fluid_height(w, [x, y, z]) {
                        let height = f64::from(y) + f64::from(h);
                        self.water_level = self.water_level.max(height);
                        in_water |= bb.min[1] < height;
                    }
                }
            }
        }
        in_water
    }

    fn ground_friction<W: CollisionWorld + ?Sized>(&self, w: &W) -> f32 {
        let bb = self.body.bounding_box();
        let probe = Aabb::new(
            [bb.min[0], bb.min[1] - 0.001, bb.min[2]],
            [bb.max[0], bb.min[1], bb.max[2]],
        );
        let (x0, x1) = (floor(probe.min[0]) - 1, ceil(probe.max[0]) + 1);
        let (y0, y1) = (floor(probe.min[1]) - 1, ceil(probe.max[1]) + 1);
        let (z0, z1) = (floor(probe.min[2]) - 1, ceil(probe.max[2]) + 1);
        let mut friction = 0.0f32;
        let mut count = 0u32;
        for x in x0..x1 {
            for z in z0..z1 {
                let edges = i32::from(x == x0 || x == x1 - 1) + i32::from(z == z0 || z == z1 - 1);
                if edges == 2 {
                    continue;
                }
                for y in y0..y1 {
                    if edges > 0 && y != y0 && y != y1 - 1 {
                        continue;
                    }
                    let s = self.body.state_at(w, [x, y, z]);
                    if s.has(flag::LILY_PAD) {
                        continue;
                    }
                    let hit = self.body.data.boxes(s).iter().any(|b| {
                        let shape = Aabb::new(
                            [
                                f64::from(x) + b[0],
                                f64::from(y) + b[1],
                                f64::from(z) + b[2],
                            ],
                            [
                                f64::from(x) + b[3],
                                f64::from(y) + b[4],
                                f64::from(z) + b[5],
                            ],
                        );
                        shape.intersects(&probe)
                    });
                    if hit {
                        friction += s.friction;
                        count += 1;
                    }
                }
            }
        }
        friction / count as f32
    }

    fn status<W: CollisionWorld + ?Sized>(&mut self, w: &W) -> BoatStatus {
        if let Some(s) = self.under_water(w) {
            self.water_level = self.body.bounding_box().max[1];
            return s;
        }
        if self.check_in_water(w) {
            return BoatStatus::InWater;
        }
        let f = self.ground_friction(w);
        if f > 0.0 {
            self.land_friction = f;
            BoatStatus::OnLand
        } else {
            BoatStatus::InAir
        }
    }

    fn water_level_above<W: CollisionWorld + ?Sized>(&self, w: &W) -> f64 {
        let bb = self.body.bounding_box();
        let (min_y, max_y) = (floor(bb.max[1]), ceil(bb.max[1] - self.last_yd));
        for y in min_y..max_y {
            let mut block_height = 0.0f32;
            let mut full = false;
            for x in floor(bb.min[0])..ceil(bb.max[0]) {
                for z in floor(bb.min[2])..ceil(bb.max[2]) {
                    if let Some((h, _)) = self.state_fluid_height(w, [x, y, z]) {
                        block_height = block_height.max(h);
                    }
                    if block_height >= 1.0 {
                        full = true;
                        break;
                    }
                }
                if full {
                    break;
                }
            }
            if !full {
                return f64::from(y) + f64::from(block_height);
            }
        }
        f64::from(max_y + 1)
    }

    fn float_boat<W: CollisionWorld + ?Sized>(&mut self, w: &W) {
        let gravity = self.body.attributes.gravity;
        let mut vspeed = -gravity;
        let mut buoyancy = 0.0f64;
        let inv_friction: f32;
        let height = f64::from(self.body.dims.map_or(0.5625, |d| d.1));
        if self.old_status == BoatStatus::InAir
            && self.status != BoatStatus::InAir
            && self.status != BoatStatus::OnLand
        {
            self.water_level = self.body.pos[1] + height;
            let target_y = self.water_level_above(w) - height + 0.101;
            let moved = self
                .body
                .bounding_box()
                .move_by([0.0, target_y - self.body.pos[1], 0.0]);
            if self.body.no_collision(w, &moved) {
                self.body.pos[1] = target_y;
                self.body.vel[1] = 0.0;
                self.last_yd = 0.0;
            }
            self.status = BoatStatus::InWater;
            return;
        }
        match self.status {
            BoatStatus::InWater => {
                buoyancy = (self.water_level - self.body.pos[1]) / height;
                inv_friction = 0.9;
            }
            BoatStatus::UnderFlowingWater => {
                vspeed = -7.0e-4;
                inv_friction = 0.9;
            }
            BoatStatus::UnderWater => {
                buoyancy = f64::from(0.01f32);
                inv_friction = 0.45;
            }
            BoatStatus::InAir => inv_friction = 0.9,
            BoatStatus::OnLand => {
                inv_friction = self.land_friction;
                self.land_friction /= 2.0;
            }
        }
        let f = f64::from(inv_friction);
        let v = self.body.vel;
        self.body.vel = [v[0] * f, v[1] + vspeed, v[2] * f];
        self.delta_rotation *= inv_friction;
        if buoyancy > 0.0 {
            let y = self.body.vel[1];
            self.body.vel[1] = (y + buoyancy * (gravity / 0.65)) * 0.75;
        }
    }

    fn control_boat(&mut self, input: &PlayerInput) {
        let (left, right, up, down) = (input.left, input.right, input.forward, input.backward);
        let mut acceleration = 0.0f32;
        if left {
            self.delta_rotation -= 1.0;
        }
        if right {
            self.delta_rotation += 1.0;
        }
        if right != left && !up && !down {
            acceleration += 0.005;
        }
        self.body.yaw += self.delta_rotation;
        if up {
            acceleration += 0.04;
        }
        if down {
            acceleration -= 0.005;
        }
        let yaw = self.body.yaw * DEG_TO_RAD;
        self.body.vel[0] += f64::from(trig::sin(f64::from(-yaw)) * acceleration);
        self.body.vel[2] += f64::from(trig::cos(f64::from(yaw)) * acceleration);
        self.paddle = [(right && !left) || up, (left && !right) || up];
    }

    /// One client tick of a boat the local player paddles.
    pub fn tick<W: CollisionWorld + ?Sized>(&mut self, world: &W, input: &PlayerInput) {
        self.old_status = self.status;
        self.status = self.status(world);
        self.body.move_from = None;
        self.body.update_fluid_interaction(world);
        self.float_boat(world);
        self.control_boat(input);
        self.last_yd = self.body.vel[1];
        let v = self.body.vel;
        self.body.do_move(world, v);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MountMode {
    Horse,
    Pig,
    Strider,
    Camel,
    HappyGhast,
}

const CAMEL_DASH_COOLDOWN: i32 = 55;

fn wrap_degrees(v: f32) -> f32 {
    let mut d = v % 360.0;
    if d >= 180.0 {
        d -= 360.0;
    }
    if d < -180.0 {
        d += 360.0;
    }
    d
}

/// A mob the player steers: horses, camels, pigs, striders and happy ghasts.
#[derive(Debug, Clone)]
pub struct Mount {
    pub body: PlayerPhysics,
    jump_pending: f32,
    pub mode: MountMode,
    dash_cooldown: i32,
    /// Sitting or changing pose: the camel will not move.
    pub refuse_to_move: bool,
}

impl Mount {
    pub fn new(pos: [f64; 3], yaw: f32, width: f32, height: f32, attributes: Attributes) -> Self {
        let mut body = PlayerPhysics::new(pos);
        body.dims = Some((width, height));
        body.yaw = yaw;
        body.attributes = attributes;
        Self {
            body,
            jump_pending: 0.0,
            mode: MountMode::Horse,
            dash_cooldown: 0,
            refuse_to_move: false,
        }
    }

    pub fn with_mode(mut self, mode: MountMode) -> Self {
        self.mode = mode;
        self.body.stand_on_lava = mode == MountMode::Strider;
        self
    }

    pub fn set_attributes(&mut self, attributes: Attributes) {
        self.body.attributes = attributes;
    }

    pub fn dash_cooldown(&self) -> i32 {
        self.dash_cooldown
    }

    /// Queues a rider jump (`jump_amount` is the 0-100 charge the client reports).
    pub fn rider_jump(&mut self, jump_amount: i32) {
        if self.mode == MountMode::Camel && (self.dash_cooldown > 0 || !self.body.on_ground) {
            return;
        }
        self.jump_pending = if jump_amount < 0 {
            0.0
        } else if jump_amount >= 90 {
            1.0
        } else {
            0.4 + 0.4 * jump_amount as f32 / 90.0
        };
    }

    pub fn snap_to(&mut self, pos: [f64; 3], yaw: f32, pitch: f32) {
        self.body.pos = pos;
        self.body.yaw = yaw;
        self.body.pitch = pitch;
    }

    fn execute_jump<W: CollisionWorld + ?Sized>(
        &mut self,
        world: &W,
        amount: f32,
        forward: f32,
        attr_speed: f32,
    ) {
        let b = &mut self.body;
        let jump_factor = f64::from(b.block_jump_factor(world));
        let boost = b
            .effects
            .jump_boost
            .map_or(0.0, |a| 0.1 * (f32::from(a) + 1.0));
        if self.mode == MountMode::Camel {
            let power = b.attributes.jump_strength as f32 * jump_factor as f32 + boost;
            let yaw = b.yaw * DEG_TO_RAD;
            let (lx, lz) = (
                f64::from(trig::sin(f64::from(-yaw))),
                f64::from(trig::cos(f64::from(yaw))),
            );
            let len = (lx * lx + lz * lz).sqrt();
            let scale = f64::from(22.2222f32)
                * f64::from(amount)
                * f64::from(attr_speed)
                * f64::from(b.block_speed_factor(world));
            if len > 0.0 {
                b.vel[0] += lx / len * scale;
                b.vel[2] += lz / len * scale;
            }
            b.vel[1] += f64::from(1.4285f32) * f64::from(amount) * f64::from(power);
            self.dash_cooldown = CAMEL_DASH_COOLDOWN;
            return;
        }
        let power = b.attributes.jump_strength as f32 * amount * jump_factor as f32 + boost;
        b.vel[1] = f64::from(power);
        if forward > 0.0 {
            let yaw = b.yaw * DEG_TO_RAD;
            b.vel[0] += f64::from(-0.4 * trig::sin(f64::from(yaw)) * amount);
            b.vel[2] += f64::from(0.4 * trig::cos(f64::from(yaw)) * amount);
        }
    }

    /// `rider` carries the rider's view and key impulses; returns the vehicle's new yaw.
    /// `attr_speed` is the mob's own movement attribute and `ridden_speed` what `getRiddenSpeed` yields.
    pub fn tick<W: CollisionWorld + ?Sized>(
        &mut self,
        world: &W,
        rider: &PlayerInput,
        attr_speed: f32,
        ridden_speed: f32,
    ) -> f32 {
        if self.mode == MountMode::HappyGhast {
            return self.tick_ghast(world, rider, ridden_speed);
        }
        let axes = PlayerPhysics::rider_axes(rider);
        let mut input = match self.mode {
            MountMode::Pig | MountMode::Strider => [0.0, 0.0, 1.0],
            _ => {
                let mut forward = axes[1];
                if forward <= 0.0 {
                    forward *= 0.25;
                }
                [f64::from(axes[0] * 0.5), 0.0, f64::from(forward)]
            }
        };
        let refuse = self.mode == MountMode::Camel && self.refuse_to_move;
        if refuse {
            input = [0.0; 3];
        }
        let b = &mut self.body;
        if !refuse {
            b.yaw = rider.yaw;
            b.pitch = rider.pitch * 0.5;
        }
        b.move_from = None;
        b.was_eye_in_water = b.fluid.eye_in_water;
        b.update_fluid_interaction(world);
        if b.on_ground {
            let amount = self.jump_pending;
            if amount > 0.0 && matches!(self.mode, MountMode::Horse | MountMode::Camel) {
                self.execute_jump(world, amount, input[2] as f32, attr_speed);
            }
            self.jump_pending = 0.0;
        }
        let b = &mut self.body;
        if refuse && b.on_ground {
            b.vel[0] = 0.0;
            b.vel[2] = 0.0;
        }
        b.attributes.movement_speed = f64::from(ridden_speed);
        b.ridden_travel(world, input);
        if self.mode == MountMode::Strider {
            self.float_strider(world);
        }
        if self.dash_cooldown > 0 {
            self.dash_cooldown -= 1;
        }
        self.body.yaw
    }

    fn float_strider<W: CollisionWorld + ?Sized>(&mut self, world: &W) {
        let b = &mut self.body;
        if !b.is_in_lava() {
            return;
        }
        let p = b.block_pos();
        let above_lava = b.state_at(world, [p[0], p[1] + 1, p[2]]).fluid == crate::data::FLUID_LAVA;
        if b.pos[1] > f64::from(p[1]) + 0.5 - 1.0e-5 && !above_lava {
            b.on_ground = true;
        } else {
            b.vel = [b.vel[0] * 0.5, b.vel[1] * 0.5 + 0.05, b.vel[2] * 0.5];
        }
    }

    fn tick_ghast<W: CollisionWorld + ?Sized>(
        &mut self,
        world: &W,
        rider: &PlayerInput,
        flying_speed: f32,
    ) -> f32 {
        let axes = PlayerPhysics::rider_axes(rider);
        let b = &mut self.body;
        let pitch = rider.pitch * DEG_TO_RAD;
        let (mut forward, mut up) = (0.0f32, 0.0f32);
        if axes[1] != 0.0 {
            let mut look_forward = trig::cos(f64::from(pitch));
            let mut look_up = -trig::sin(f64::from(pitch));
            if axes[1] < 0.0 {
                look_forward *= -0.5;
                look_up *= -0.5;
            }
            forward = look_forward;
            up = look_up;
        }
        if rider.jump {
            up += 0.5;
        }
        let scale = f64::from(3.9f32) * f64::from(flying_speed);
        let input = [
            f64::from(axes[0]) * scale,
            f64::from(up) * scale,
            f64::from(forward) * scale,
        ];
        b.yaw += wrap_degrees(rider.yaw - b.yaw) * 0.08;
        b.pitch = rider.pitch * 0.5;
        b.move_from = None;
        b.was_eye_in_water = b.fluid.eye_in_water;
        b.update_fluid_interaction(world);
        let speed = flying_speed * 5.0 / 3.0;
        b.move_relative(speed, input);
        let v = b.vel;
        b.do_move(world, v);
        let drag = if b.is_in_water() {
            f64::from(0.8f32)
        } else if b.is_in_lava() {
            0.5
        } else {
            f64::from(0.91f32)
        };
        b.vel = [b.vel[0] * drag, b.vel[1] * drag, b.vel[2] * drag];
        b.yaw
    }
}
