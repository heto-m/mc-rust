use crate::aabb::Aabb;
use crate::collide::{any_collision, find_supporting_block};
use crate::data::{StateInfo, flag};
use crate::player::PlayerPhysics;
use crate::trig;
use crate::world::CollisionWorld;

const F_1E5: f64 = 1.0e-5_f32 as f64;

fn len_sqr(v: [f64; 3]) -> f64 {
    v[0] * v[0] + v[1] * v[1] + v[2] * v[2]
}

fn near(a: f64, b: f64) -> bool {
    (b - a).abs() < F_1E5
}

fn computed_friction(friction: f32, modifier: f32) -> f32 {
    (1.0 - (1.0 - friction) * modifier).clamp(0.0, 1.0)
}

impl PlayerPhysics {
    pub(crate) fn block_friction<W: CollisionWorld + ?Sized>(&self, w: &W) -> f32 {
        let below = self.block_below_affecting_movement(w);
        let f = self.state_at(w, below).friction;
        computed_friction(f, self.attributes.friction_modifier as f32)
    }

    pub(crate) fn air_drag(&self, base: f32) -> f32 {
        computed_friction(base, self.attributes.air_drag_modifier as f32)
    }

    pub(crate) fn block_speed_factor<W: CollisionWorld + ?Sized>(&self, w: &W) -> f32 {
        if self.abilities.flying {
            return 1.0;
        }
        let here = self.state_at(w, self.block_pos());
        let base = if here.has(flag::WATER_BLOCK) || here.speed_factor != 1.0 {
            here.speed_factor
        } else {
            let below = self.block_below_affecting_movement(w);
            self.state_at(w, below).speed_factor
        };
        let t = self.attributes.movement_efficiency as f32;
        base + t * (1.0 - base)
    }

    pub(crate) fn block_jump_factor<W: CollisionWorld + ?Sized>(&self, w: &W) -> f32 {
        let here = self.state_at(w, self.block_pos()).jump_factor;
        if here == 1.0 {
            let below = self.block_below_affecting_movement(w);
            self.state_at(w, below).jump_factor
        } else {
            here
        }
    }

    pub(crate) fn no_collision<W: CollisionWorld + ?Sized>(&self, w: &W, bb: &Aabb) -> bool {
        !any_collision(w, self.data, bb, &self.ctx())
    }

    pub(crate) fn check_supporting_block<W: CollisionWorld + ?Sized>(
        &mut self,
        w: &W,
        on_ground: bool,
        movement: Option<[f64; 3]>,
    ) {
        if !on_ground {
            self.on_ground_no_blocks = false;
            self.support = None;
            return;
        }
        let bb = self.bounding_box();
        let test = Aabb::new(
            [bb.min[0], bb.min[1] - 1.0e-6, bb.min[2]],
            [bb.max[0], bb.min[1], bb.max[2]],
        );
        let ctx = self.ctx();
        let mut found = find_supporting_block(w, self.data, &test, self.pos, &ctx);
        if found.is_some() || self.on_ground_no_blocks {
            self.support = found;
        } else if let Some(m) = movement {
            let t2 = test.move_by([-m[0], 0.0, -m[2]]);
            found = find_supporting_block(w, self.data, &t2, self.pos, &ctx);
            self.support = found;
        }
        self.on_ground_no_blocks = found.is_none();
    }

    pub(crate) fn set_on_ground<W: CollisionWorld + ?Sized>(&mut self, w: &W, on_ground: bool) {
        self.on_ground = on_ground;
        self.check_supporting_block(w, on_ground, None);
    }

    fn can_fall_at_least<W: CollisionWorld + ?Sized>(
        &self,
        w: &W,
        dx: f64,
        dz: f64,
        min_height: f64,
    ) -> bool {
        let bb = self.bounding_box();
        let region = Aabb::new(
            [
                bb.min[0] + 1.0e-7 + dx,
                bb.min[1] - min_height - 1.0e-7,
                bb.min[2] + 1.0e-7 + dz,
            ],
            [bb.max[0] - 1.0e-7 + dx, bb.min[1], bb.max[2] - 1.0e-7 + dz],
        );
        self.no_collision(w, &region)
    }

    fn is_above_ground<W: CollisionWorld + ?Sized>(&self, w: &W, max_down: f64) -> bool {
        self.on_ground
            || (self.fall_distance < max_down
                && !self.can_fall_at_least(w, 0.0, 0.0, max_down - self.fall_distance))
    }

    fn back_off_from_edge<W: CollisionWorld + ?Sized>(&self, w: &W, delta: [f64; 3]) -> [f64; 3] {
        let max_down = f64::from(self.max_up_step());
        if self.abilities.flying
            || delta[1] > 0.0
            || !self.keys.sneak
            || !self.is_above_ground(w, max_down)
        {
            return delta;
        }
        let (mut dx, mut dz) = (delta[0], delta[2]);
        let sx = signum(dx) * 0.05;
        let sz = signum(dz) * 0.05;
        while dx != 0.0 && self.can_fall_at_least(w, dx, 0.0, max_down) {
            if dx.abs() <= 0.05 {
                dx = 0.0;
                break;
            }
            dx -= sx;
        }
        while dz != 0.0 && self.can_fall_at_least(w, 0.0, dz, max_down) {
            if dz.abs() <= 0.05 {
                dz = 0.0;
                break;
            }
            dz -= sz;
        }
        while dx != 0.0 && dz != 0.0 && self.can_fall_at_least(w, dx, dz, max_down) {
            if dx.abs() <= 0.05 {
                dx = 0.0;
            } else {
                dx -= sx;
            }
            if dz.abs() <= 0.05 {
                dz = 0.0;
            } else {
                dz -= sz;
            }
        }
        [dx, delta[1], dz]
    }

    fn collide<W: CollisionWorld + ?Sized>(&mut self, w: &W, movement: [f64; 3]) -> [f64; 3] {
        let bb = self.bounding_box();
        let max_up = self.max_up_step();
        let hull = bb
            .expand_towards(movement)
            .expand_towards([0.0, f64::from(max_up), 0.0])
            .expand_towards([0.0, -F_1E5, 0.0]);
        let ctx = self.ctx();
        self.colliders.gather(w, self.data, &hull, &ctx);
        let step = if len_sqr(movement) == 0.0 {
            movement
        } else {
            self.colliders.collide_with_shapes(movement, &bb)
        };
        let xc = movement[0] != step[0];
        let yc = movement[1] != step[1];
        let zc = movement[2] != step[2];
        let grounded_after = yc && movement[1] < 0.0;
        if max_up > 0.0 && (grounded_after || self.on_ground) && (xc || zc) {
            let grounded = if grounded_after {
                bb.move_by([0.0, step[1], 0.0])
            } else {
                bb
            };
            let mut up = grounded.expand_towards([movement[0], f64::from(max_up), movement[2]]);
            if !grounded_after {
                up = up.expand_towards([0.0, -F_1E5, 0.0]);
            }
            let mut cands = std::mem::take(&mut self.step_scratch);
            self.colliders.step_candidates(
                &up,
                grounded.min[1],
                max_up,
                step[1] as f32,
                &mut cands,
            );
            let base_h = step[0] * step[0] + step[2] * step[2];
            let mut result = None;
            for &c in &cands {
                let s = self
                    .colliders
                    .collide_with_shapes([movement[0], f64::from(c), movement[2]], &grounded);
                if s[0] * s[0] + s[2] * s[2] > base_h {
                    result = Some([s[0], s[1] - (bb.min[1] - grounded.min[1]), s[2]]);
                    break;
                }
            }
            self.step_scratch = cands;
            if let Some(r) = result {
                return r;
            }
        }
        step
    }

    pub(crate) fn do_move<W: CollisionWorld + ?Sized>(&mut self, w: &W, mut delta: [f64; 3]) {
        if self.spectator {
            for (p, d) in self.pos.iter_mut().zip(delta) {
                *p += d;
            }
            self.horizontal_collision = false;
            self.vertical_collision = false;
            self.vertical_collision_below = false;
            self.minor_horizontal_collision = false;
            return;
        }
        if self.move_from.is_none() {
            self.move_from = Some(self.pos);
        }
        if len_sqr(self.stuck) > 1.0e-7 {
            delta = [
                delta[0] * self.stuck[0],
                delta[1] * self.stuck[1],
                delta[2] * self.stuck[2],
            ];
            self.stuck = [0.0; 3];
            self.vel = [0.0; 3];
        }
        delta = self.back_off_from_edge(w, delta);
        let movement = self.collide(w, delta);
        let ml = len_sqr(movement);
        if ml > 1.0e-7 || len_sqr(delta) - ml < 1.0e-7 {
            for (p, m) in self.pos.iter_mut().zip(movement) {
                *p += m;
            }
        }
        let xc = !near(delta[0], movement[0]);
        let zc = !near(delta[2], movement[2]);
        self.horizontal_collision = xc || zc;
        let moved_vertically = delta[1].abs() > 0.0;
        self.vertical_collision = delta[1] != movement[1];
        self.vertical_collision_below = self.vertical_collision && delta[1] < 0.0;
        self.on_ground = self.vertical_collision_below;
        self.check_supporting_block(w, self.on_ground, Some(movement));
        self.minor_horizontal_collision =
            self.horizontal_collision && self.is_horizontal_collision_minor(movement);
        let effect_pos = self.on_ground_pos(0.2, w);
        let effect_state = *self.state_at(w, effect_pos);
        self.check_fall_damage(w, movement[1]);
        if (moved_vertically && self.vertical_collision) || self.horizontal_collision {
            self.restitute(&effect_state, xc, zc, movement);
        }
        let f = f64::from(self.block_speed_factor(w));
        self.vel[0] *= f;
        self.vel[2] *= f;
    }

    fn check_fall_damage<W: CollisionWorld + ?Sized>(&mut self, w: &W, ya: f64) {
        if !self.is_in_water() {
            self.update_fluid_interaction(w);
        }
        if !self.is_in_water() && ya < 0.0 {
            self.fall_distance -= f64::from(ya as f32);
        }
        if self.on_ground {
            self.fall_distance = 0.0;
        }
    }

    fn restitute(&mut self, effect: &StateInfo, xc: bool, zc: bool, movement: [f64; 3]) {
        let sneaking = self.keys.sneak;
        let mut restitution = 0.0f64;
        let cur = self.vel;
        let mut after = cur;
        if xc {
            after[0] = -cur[0] * restitution;
        }
        if zc {
            after[2] = -cur[2] * restitution;
        }
        if self.vertical_collision {
            let gravity = self.effective_gravity();
            if self.vertical_collision_below {
                restitution = if -cur[1] > gravity && !sneaking && !effect.has(flag::HONEY) {
                    restitution.max(f64::from(effect.bounce))
                } else {
                    0.0
                };
            }
            let (comp, drag) = if restitution > 0.0 {
                let portion = movement[1] / cur[1];
                let drag_air = f64::from(self.air_drag(0.98));
                (portion * gravity, 1.0 + portion * (drag_air - 1.0))
            } else {
                (0.0, 1.0)
            };
            after[1] = (comp - cur[1]) * drag * restitution;
        }
        self.vel = after;
    }

    fn is_horizontal_collision_minor(&self, movement: [f64; 3]) -> bool {
        let rad = self.yaw * crate::trig::DEG_TO_RAD;
        let s = f64::from(trig::sin(f64::from(rad)));
        let c = f64::from(trig::cos(f64::from(rad)));
        let xxa = f64::from(self.move_vec[0]);
        let zza = f64::from(self.move_vec[1]);
        let gx = xxa * c - zza * s;
        let gz = zza * c + xxa * s;
        let a_len = gx * gx + gz * gz;
        let m_len = movement[0] * movement[0] + movement[2] * movement[2];
        if a_len >= F_1E5 && m_len >= F_1E5 {
            let dot = gx * movement[0] + gz * movement[2];
            let angle = (dot / (a_len * m_len).sqrt()).acos();
            angle < f64::from(0.13962634f32)
        } else {
            false
        }
    }
}

fn signum(x: f64) -> f64 {
    if x == 0.0 || x.is_nan() {
        x
    } else {
        x.signum()
    }
}
