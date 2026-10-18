use crate::aabb::Aabb;
use crate::collide::any_suffocating;
use crate::data::{FLUID_NONE, flag};
use crate::player::{PlayerPhysics, SPRINT_WINDOW};
use crate::trig::{self, DEG_TO_RAD};
use crate::types::{MovementOutput, PlayerInput, Pose};
use crate::world::CollisionWorld;

const F_0_6: f64 = 0.6;

fn impulse(positive: bool, negative: bool) -> f32 {
    if positive == negative {
        0.0
    } else if positive {
        1.0
    } else {
        -1.0
    }
}

fn normalized(x: f32, y: f32) -> [f32; 2] {
    let d = (x * x + y * y).sqrt();
    if d < 1.0e-4 {
        [0.0, 0.0]
    } else {
        [x / d, y / d]
    }
}

fn square_movement(v: [f32; 2]) -> [f32; 2] {
    let len = (v[0] * v[0] + v[1] * v[1]).sqrt();
    if len <= 0.0 {
        return v;
    }
    let k = 1.0 / len;
    let dir = [v[0] * k, v[1] * k];
    let (ax, ay) = (dir[0].abs(), dir[1].abs());
    let tan = if ay > ax { ax / ay } else { ay / ax };
    let dist = (1.0 + tan * tan).sqrt();
    let m = (len * dist).min(1.0);
    [dir[0] * m, dir[1] * m]
}

impl PlayerPhysics {
    pub fn tick<W: CollisionWorld + ?Sized>(
        &mut self,
        input: &PlayerInput,
        world: &W,
    ) -> MovementOutput {
        self.yaw = input.yaw;
        self.pitch = input.pitch;
        if self.spectator {
            self.set_on_ground(world, false);
        }
        self.move_from = None;
        self.was_in_powder_snow = self.in_powder_snow;
        self.in_powder_snow = false;
        self.was_eye_in_water = self.fluid.eye_in_water;
        self.update_fluid_interaction(world);
        self.update_swimming(world);
        if self.is_in_lava() {
            self.fall_distance *= 0.5;
        }
        self.ai_step(input, world);
        self.pos[0] = self.pos[0].clamp(-2.9999999e7, 2.9999999e7);
        self.pos[2] = self.pos[2].clamp(-2.9999999e7, 2.9999999e7);
        self.update_pose(world);
        self.build_output(input)
    }

    fn can_fit<W: CollisionWorld + ?Sized>(&self, w: &W, pose: Pose) -> bool {
        let bb = self.bounding_box_for(pose, self.pos).inflate(-1.0e-7);
        self.no_collision(w, &bb)
    }

    fn update_pose<W: CollisionWorld + ?Sized>(&mut self, w: &W) {
        if !self.can_fit(w, Pose::Swimming) {
            return;
        }
        let desired = if self.sleeping {
            Pose::Sleeping
        } else if self.swimming {
            Pose::Swimming
        } else if self.fall_flying {
            Pose::FallFlying
        } else if self.spin_attack_flag {
            Pose::SpinAttack
        } else if self.keys.sneak && !self.abilities.flying {
            Pose::Crouching
        } else {
            Pose::Standing
        };
        self.pose = if self.spectator || self.can_fit(w, desired) {
            desired
        } else if self.can_fit(w, Pose::Crouching) {
            Pose::Crouching
        } else {
            Pose::Swimming
        };
    }

    fn is_sprinting_possible(&self, allow_shallow: bool) -> bool {
        let has_food = self.food_level > 6 || self.abilities.may_fly;
        has_food && (allow_shallow || !(self.is_in_water() && !self.is_under_water()))
    }

    fn ai_step<W: CollisionWorld + ?Sized>(&mut self, input: &PlayerInput, w: &W) {
        if self.sprint_trigger_time > 0 {
            self.sprint_trigger_time -= 1;
        }
        let was_jumping = self.keys.jump;
        let was_shift = self.keys.sneak;
        let had_forward = self.move_vec[1] > 1.0e-5;
        self.crouching = !self.abilities.flying
            && !self.swimming
            && self.can_fit(w, Pose::Crouching)
            && (self.keys.sneak || !self.can_fit(w, Pose::Standing));
        self.keys = *input;
        if !self.spectator && !self.passenger {
            let bb = self.bounding_box();
            let reach = (bb.max[0] - bb.min[0]) * 0.35;
            let [x, _, z] = self.pos;
            for (sx, sz) in [(-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0), (1.0, 1.0)] {
                self.move_towards_closest_space(w, x + sx * reach, z + sz * reach);
            }
        }
        let mv = normalized(
            impulse(input.left, input.right),
            impulse(input.forward, input.backward),
        );
        self.move_vec = mv;
        let has_forward = mv[1] > 1.0e-5;
        if was_shift || self.slow_due_to_item() || input.backward {
            self.sprint_trigger_time = 0;
        }
        let can_start = !self.sprinting
            && has_forward
            && self.is_sprinting_possible(self.abilities.flying)
            && !self.slow_due_to_item()
            && (!self.fall_flying || self.is_under_water())
            && !self.is_moving_slowly_or_under();
        if can_start {
            if !had_forward {
                if self.sprint_trigger_time > 0 {
                    self.sprinting = true;
                } else {
                    self.sprint_trigger_time = SPRINT_WINDOW;
                }
            }
            if input.sprint {
                self.sprinting = true;
            }
        }
        if self.sprinting {
            if self.swimming {
                if !self.is_sprinting_possible(true)
                    || !self.is_in_water()
                    || (!has_forward && !self.on_ground && !input.sneak)
                {
                    self.sprinting = false;
                }
            } else if !self.is_sprinting_possible(self.abilities.flying)
                || !has_forward
                || (self.horizontal_collision && !self.minor_horizontal_collision)
            {
                self.sprinting = false;
            }
        }
        let mut just_toggled = false;
        if self.abilities.may_fly {
            if self.spectator {
                if !self.abilities.flying {
                    self.abilities.flying = true;
                    just_toggled = true;
                    self.abilities_changed = true;
                }
            } else if !was_jumping && input.jump {
                if self.jump_trigger_time == 0 {
                    self.jump_trigger_time = 7;
                } else if !self.swimming {
                    self.abilities.flying = !self.abilities.flying;
                    if self.abilities.flying && self.on_ground {
                        self.jump_from_ground(w);
                    }
                    just_toggled = true;
                    self.abilities_changed = true;
                    self.jump_trigger_time = 0;
                }
            }
        }
        if input.jump
            && !just_toggled
            && !was_jumping
            && !self.on_climbable(w)
            && self.try_start_fall_flying()
        {
            self.fall_flying_started = true;
        }
        if self.is_in_water() && input.sneak && !self.abilities.flying {
            self.vel[1] += f64::from(-0.04f32);
        }
        if self.abilities.flying {
            let dir = i32::from(input.jump) - i32::from(input.sneak);
            if dir != 0 {
                let dy = (dir as f32 * self.abilities.flying_speed) * 3.0;
                self.vel[1] += f64::from(dy);
            }
        }
        if self.jump_trigger_time > 0 {
            self.jump_trigger_time -= 1;
        }
        if self.abilities.flying {
            self.fall_distance = 0.0;
        }
        self.living_ai_step(input, w);
        if self.on_ground && self.abilities.flying && !self.spectator {
            self.abilities.flying = false;
            self.abilities_changed = true;
        }
    }

    fn suffocates_at<W: CollisionWorld + ?Sized>(&self, w: &W, x: i32, z: i32) -> bool {
        let bb = self.bounding_box();
        let area = Aabb::new(
            [f64::from(x), bb.min[1], f64::from(z)],
            [f64::from(x) + 1.0, bb.max[1], f64::from(z) + 1.0],
        )
        .inflate(-1.0e-7);
        any_suffocating(w, self.data, &area)
    }

    /// Vanilla `LocalPlayer#moveTowardsClosestSpace`: nudges the player out of a suffocating block.
    fn move_towards_closest_space<W: CollisionWorld + ?Sized>(&mut self, w: &W, x: f64, z: f64) {
        let (bx, bz) = (x.floor() as i32, z.floor() as i32);
        if !self.suffocates_at(w, bx, bz) {
            return;
        }
        let (xd, zd) = (x - f64::from(bx), z - f64::from(bz));
        let candidates = [(xd, -1, 0), (1.0 - xd, 1, 0), (zd, 0, -1), (1.0 - zd, 0, 1)];
        let mut best: Option<(i32, i32)> = None;
        let mut closest = f64::MAX;
        for (distance, dx, dz) in candidates {
            if distance < closest && !self.suffocates_at(w, bx + dx, bz + dz) {
                closest = distance;
                best = Some((dx, dz));
            }
        }
        match best {
            Some((dx, 0)) => self.vel[0] = 0.1 * f64::from(dx),
            Some((_, dz)) => self.vel[2] = 0.1 * f64::from(dz),
            None => {}
        }
    }

    /// Vanilla `Entity#push(Entity)` as a remote entity at `from` shoves the local player.
    pub fn push_from<W: CollisionWorld + ?Sized>(&mut self, w: &W, from: [f64; 3]) {
        if self.spectator || self.passenger || self.on_climbable(w) {
            return;
        }
        let (mut xa, mut za) = (from[0] - self.pos[0], from[2] - self.pos[2]);
        let mut dd = xa.abs().max(za.abs());
        if !dd.is_finite() || dd < f64::from(0.01f32) {
            return;
        }
        dd = dd.sqrt();
        xa /= dd;
        za /= dd;
        let pow = (1.0 / dd).min(1.0);
        xa *= pow;
        za *= pow;
        xa *= f64::from(0.05f32);
        za *= f64::from(0.05f32);
        self.vel[0] -= xa;
        self.vel[2] -= za;
    }

    fn slow_due_to_item(&self) -> bool {
        self.using_item.is_some_and(|u| !u.can_sprint) && !self.passenger
    }

    fn is_moving_slowly_or_under(&self) -> bool {
        self.is_moving_slowly() && !self.is_under_water()
    }

    fn modify_input(&self) -> [f32; 2] {
        let v = self.move_vec;
        if v[0] * v[0] + v[1] * v[1] == 0.0 {
            return v;
        }
        let mut n = [v[0] * 0.98, v[1] * 0.98];
        if let Some(u) = self.using_item.filter(|_| !self.passenger) {
            n = [n[0] * u.speed_multiplier, n[1] * u.speed_multiplier];
        }
        if self.is_moving_slowly() {
            let f = self.attributes.sneaking_speed as f32;
            n = [n[0] * f, n[1] * f];
        }
        square_movement(n)
    }

    /// `xxa`/`zza` of a rider: impulses scaled by 0.98 and squared like the local player's input.
    pub fn rider_axes(input: &PlayerInput) -> [f32; 2] {
        let mv = normalized(
            impulse(input.left, input.right),
            impulse(input.forward, input.backward),
        );
        if mv[0] * mv[0] + mv[1] * mv[1] == 0.0 {
            return mv;
        }
        square_movement([mv[0] * 0.98, mv[1] * 0.98])
    }

    /// Travel of a controlled non-player mob: the living-entity step without jump or sprint logic.
    pub(crate) fn ridden_travel<W: CollisionWorld + ?Sized>(&mut self, w: &W, input: [f64; 3]) {
        let (mut dx, mut dz) = (self.vel[0], self.vel[2]);
        if dx * dx + dz * dz < 9.0e-6 {
            dx = 0.0;
            dz = 0.0;
        }
        let dy = if self.vel[1].abs() < 0.003 {
            0.0
        } else {
            self.vel[1]
        };
        self.vel = [dx, dy, dz];
        self.jumping = false;
        self.sprinting = false;
        if self.effects.slow_falling || self.effects.levitation.is_some() {
            self.fall_distance = 0.0;
        }
        self.travel(w, input);
        self.apply_effects_from_blocks(w);
    }

    fn living_ai_step<W: CollisionWorld + ?Sized>(&mut self, input: &PlayerInput, w: &W) {
        if self.no_jump_delay > 0 {
            self.no_jump_delay -= 1;
        }
        let (mut dx, mut dz) = (self.vel[0], self.vel[2]);
        if dx * dx + dz * dz < 9.0e-6 {
            dx = 0.0;
            dz = 0.0;
        }
        let dy = if self.vel[1].abs() < 0.003 {
            0.0
        } else {
            self.vel[1]
        };
        self.vel = [dx, dy, dz];
        let mi = self.modify_input();
        self.jumping = input.jump;
        if self.jumping && !self.abilities.flying {
            let in_lava = self.is_in_lava();
            let fluid_height = if in_lava {
                self.fluid.lava_height
            } else {
                self.fluid.water_height
            };
            let in_water_h = self.is_in_water() && fluid_height > 0.0;
            let threshold = self.fluid_jump_threshold();
            if !in_water_h || (self.on_ground && fluid_height <= threshold) {
                if !in_lava || (self.on_ground && self.fluid.lava_height <= threshold) {
                    if (self.on_ground || (in_water_h && fluid_height <= threshold))
                        && self.no_jump_delay == 0
                    {
                        self.jump_from_ground(w);
                        self.no_jump_delay = 10;
                    }
                } else {
                    self.vel[1] += f64::from(0.04f32);
                }
            } else {
                self.vel[1] += f64::from(0.04f32);
            }
        } else {
            self.no_jump_delay = 0;
        }
        if self.fall_flying {
            self.check_fall_distance_accumulation();
        }
        if self.effects.slow_falling || self.effects.levitation.is_some() {
            self.fall_distance = 0.0;
        }
        self.travel(w, [f64::from(mi[0]), 0.0, f64::from(mi[1])]);
        self.apply_effects_from_blocks(w);
        self.tick_spin_attack();
    }

    pub(crate) fn jump_from_ground<W: CollisionWorld + ?Sized>(&mut self, w: &W) {
        let boost = self
            .effects
            .jump_boost
            .map_or(0.0f32, |a| 0.1 * (f32::from(a) + 1.0));
        let power = self.attributes.jump_strength as f32 * 1.0 * self.block_jump_factor(w) + boost;
        if power <= 1.0e-5 {
            return;
        }
        self.vel[1] = f64::from(power).max(self.vel[1]);
        if self.sprinting {
            let angle = self.yaw * DEG_TO_RAD;
            self.vel[0] += f64::from(-trig::sin(f64::from(angle))) * 0.2;
            self.vel[2] += f64::from(trig::cos(f64::from(angle))) * 0.2;
        }
    }

    pub(crate) fn move_relative(&mut self, speed: f32, input: [f64; 3]) {
        let len = input[0] * input[0] + input[1] * input[1] + input[2] * input[2];
        if len < 1.0e-7 {
            return;
        }
        let mut m = input;
        if len > 1.0 {
            let d = len.sqrt();
            m = if d < f64::from(1.0e-5f32) {
                [0.0; 3]
            } else {
                [m[0] / d, m[1] / d, m[2] / d]
            };
        }
        let s = f64::from(speed);
        m = [m[0] * s, m[1] * s, m[2] * s];
        let rad = f64::from(self.yaw * DEG_TO_RAD);
        let sin = f64::from(trig::sin(rad));
        let cos = f64::from(trig::cos(rad));
        self.vel[0] += m[0] * cos - m[2] * sin;
        self.vel[1] += m[1];
        self.vel[2] += m[2] * cos + m[0] * sin;
    }

    pub(crate) fn on_climbable<W: CollisionWorld + ?Sized>(&self, w: &W) -> bool {
        if self.spectator || self.abilities.flying {
            return false;
        }
        let p = self.block_pos();
        let st = self.state_at(w, p);
        if st.has(flag::CLIMBABLE) {
            return true;
        }
        if st.has(flag::OPEN_TRAPDOOR) {
            let below = self.state_at(w, [p[0], p[1] - 1, p[2]]);
            return below.has(flag::LADDER) && below.facing == st.facing;
        }
        false
    }

    fn handle_on_climbable<W: CollisionWorld + ?Sized>(&mut self, w: &W) {
        if !self.on_climbable(w) {
            return;
        }
        self.fall_distance = 0.0;
        let max = f64::from(0.15f32);
        let xd = self.vel[0].clamp(-max, max);
        let zd = self.vel[2].clamp(-max, max);
        let mut yd = self.vel[1].max(-max);
        let in_state = self.state_at(w, self.block_pos());
        if yd < 0.0 && !in_state.has(flag::SCAFFOLDING) && self.keys.sneak {
            yd = 0.0;
        }
        self.vel = [xd, yd, zd];
    }

    fn friction_influenced_speed(&self, friction: f32) -> f32 {
        if self.on_ground {
            if f64::from(friction) > F_0_6 {
                self.speed() * (0.21600002 / (friction * friction * friction))
            } else {
                self.speed()
            }
        } else {
            self.flying_speed()
        }
    }

    fn travel<W: CollisionWorld + ?Sized>(&mut self, w: &W, input: [f64; 3]) {
        if self.swimming {
            let x = self.pitch * DEG_TO_RAD;
            let look_y = f64::from(-trig::sin(f64::from(x)));
            let mult = if look_y < -0.2 { 0.085 } else { 0.06 };
            let p = self.pos;
            let above = self.state_at(
                w,
                [
                    p[0].floor() as i32,
                    (p[1] + 1.0 - 0.1).floor() as i32,
                    p[2].floor() as i32,
                ],
            );
            if look_y <= 0.0 || self.jumping || above.fluid != FLUID_NONE {
                self.vel[1] += (look_y - self.vel[1]) * mult;
            }
        }
        if self.abilities.flying {
            let orig = self.vel[1];
            self.travel_base(w, input);
            self.vel[1] = orig * 0.6;
        } else {
            self.travel_base(w, input);
        }
    }

    fn travel_base<W: CollisionWorld + ?Sized>(&mut self, w: &W, input: [f64; 3]) {
        if (self.is_in_water() || (self.is_in_lava() && !self.stand_on_lava))
            && !self.abilities.flying
        {
            let is_falling = self.vel[1] <= 0.0;
            let old_y = self.pos[1];
            let g = self.effective_gravity();
            if self.is_in_water() {
                self.travel_in_water(w, input, g, is_falling, old_y);
            } else {
                self.travel_in_lava(w, input, g, is_falling, old_y);
            }
        } else if self.fall_flying {
            self.travel_fall_flying(w, input);
        } else {
            self.travel_in_air(w, input);
        }
    }

    pub(crate) fn travel_in_air<W: CollisionWorld + ?Sized>(&mut self, w: &W, input: [f64; 3]) {
        let below = self.block_below_affecting_movement(w);
        let block_friction = if self.on_ground {
            self.block_friction(w)
        } else {
            1.0
        };
        self.move_relative(self.friction_influenced_speed(block_friction), input);
        self.handle_on_climbable(w);
        let v = self.vel;
        self.do_move(w, v);
        let mut movement = self.vel;
        if (self.horizontal_collision || self.jumping)
            && (self.on_climbable(w) || (self.was_in_powder_snow && self.leather_boots))
        {
            movement[1] = 0.2;
        }
        let mut my = movement[1];
        if let Some(a) = self.effects.levitation {
            my += (0.05 * (f64::from(a) + 1.0) - movement[1]) * 0.2;
        } else if w.is_loaded(below[0], below[2]) {
            my -= self.effective_gravity();
        } else if self.pos[1] > f64::from(w.min_y()) {
            my = -0.1;
        } else {
            my = 0.0;
        }
        let air = self.air_drag(0.91);
        let friction = block_friction * air;
        let vertical = self.air_drag(0.98);
        self.vel = [
            movement[0] * f64::from(friction),
            my * f64::from(vertical),
            movement[2] * f64::from(friction),
        ];
    }

    fn fluid_falling_adjusted(&self, base_gravity: f64, is_falling: bool, m: [f64; 3]) -> [f64; 3] {
        if base_gravity != 0.0 && !self.sprinting {
            let yd = if is_falling
                && (m[1] - 0.005).abs() >= 0.003
                && (m[1] - base_gravity / 16.0).abs() < 0.003
            {
                -0.003
            } else {
                m[1] - base_gravity / 16.0
            };
            [m[0], yd, m[2]]
        } else {
            m
        }
    }

    fn travel_in_water<W: CollisionWorld + ?Sized>(
        &mut self,
        w: &W,
        input: [f64; 3],
        base_gravity: f64,
        is_falling: bool,
        old_y: f64,
    ) {
        let mut slow_down: f32 = if self.sprinting { 0.9 } else { 0.8 };
        let mut speed = 0.02f32;
        let mut ww = self.attributes.water_movement_efficiency as f32;
        if !self.on_ground {
            ww *= 0.5;
        }
        if ww > 0.0 {
            slow_down += (0.54600006 - slow_down) * ww;
            speed += (self.speed() - speed) * ww;
        }
        self.move_relative(speed, input);
        let v = self.vel;
        self.do_move(w, v);
        let mut m = self.vel;
        if self.horizontal_collision && self.on_climbable(w) {
            m[1] = 0.2;
        }
        let sd = f64::from(slow_down);
        m = [m[0] * sd, m[1] * f64::from(0.8f32), m[2] * sd];
        self.vel = self.fluid_falling_adjusted(base_gravity, is_falling, m);
        self.jump_out_of_fluid(w, old_y);
    }

    fn travel_in_lava<W: CollisionWorld + ?Sized>(
        &mut self,
        w: &W,
        input: [f64; 3],
        base_gravity: f64,
        is_falling: bool,
        old_y: f64,
    ) {
        self.move_relative(0.02, input);
        let v = self.vel;
        self.do_move(w, v);
        if self.fluid.lava_height <= self.fluid_jump_threshold() {
            self.vel = [
                self.vel[0] * 0.5,
                self.vel[1] * f64::from(0.8f32),
                self.vel[2] * 0.5,
            ];
            self.vel = self.fluid_falling_adjusted(base_gravity, is_falling, self.vel);
        } else {
            self.vel = [self.vel[0] * 0.5, self.vel[1] * 0.5, self.vel[2] * 0.5];
        }
        if base_gravity != 0.0 {
            self.vel[1] += -base_gravity / 4.0;
        }
        self.jump_out_of_fluid(w, old_y);
    }

    fn jump_out_of_fluid<W: CollisionWorld + ?Sized>(&mut self, w: &W, old_y: f64) {
        let m = self.vel;
        if !self.horizontal_collision {
            return;
        }
        let dy = m[1] + f64::from(0.6f32) - self.pos[1] + old_y;
        let bb = self.bounding_box().move_by([m[0], dy, m[2]]);
        if self.no_collision(w, &bb) && !self.contains_any_liquid(w, &bb) {
            self.vel[1] = f64::from(0.3f32);
        }
    }

    fn contains_any_liquid<W: CollisionWorld + ?Sized>(
        &self,
        w: &W,
        bb: &crate::aabb::Aabb,
    ) -> bool {
        let lo = [
            bb.min[0].floor() as i32,
            bb.min[1].floor() as i32,
            bb.min[2].floor() as i32,
        ];
        let hi = [
            bb.max[0].floor() as i32,
            bb.max[1].floor() as i32,
            bb.max[2].floor() as i32,
        ];
        for x in lo[0]..=hi[0] {
            for y in lo[1]..=hi[1] {
                for z in lo[2]..=hi[2] {
                    if self.state_at(w, [x, y, z]).fluid != FLUID_NONE {
                        return true;
                    }
                }
            }
        }
        false
    }
}
