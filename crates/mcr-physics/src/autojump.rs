//! `LocalPlayer#updateAutoJump`: queues a jump when the player walks into a step a jump clears.

use crate::aabb::Aabb;
use crate::collide::cell_shape;
use crate::player::{PlayerPhysics, SimKind};
use crate::trig::{self, DEG_TO_RAD};
use crate::world::CollisionWorld;

const LOOK_AHEAD_STEPS: f32 = 7.0;
const BASE_JUMP_HEIGHT: f32 = 1.2;

/// The box spanned by two points, as `AABB#intersects(Vec3, Vec3)` tests a segment.
fn span(a: [f64; 3], b: [f64; 3]) -> Aabb {
    Aabb::new(
        std::array::from_fn(|i| a[i].min(b[i])),
        std::array::from_fn(|i| a[i].max(b[i])),
    )
}

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn scale(a: [f64; 3], k: f64) -> [f64; 3] {
    [a[0] * k, a[1] * k, a[2] * k]
}

impl PlayerPhysics {
    fn can_auto_jump<W: CollisionWorld + ?Sized>(&self, w: &W) -> bool {
        let mv = self.move_vec;
        self.auto_jump
            && self.kind == SimKind::Player
            && self.auto_jump_time <= 0
            && self.on_ground
            && !self.keys.sneak
            && !self.passenger
            && mv[0] * mv[0] + mv[1] * mv[1] > 0.0
            && self.block_jump_factor(w) >= 1.0
    }

    /// `Entity#getForward`: the full look direction, pitch included.
    fn forward(&self) -> [f64; 3] {
        let pi = std::f32::consts::PI;
        let y = f64::from(-self.yaw * DEG_TO_RAD - pi);
        let x = f64::from(-self.pitch * DEG_TO_RAD);
        let x_cos = -trig::cos(x);
        [
            f64::from(trig::sin(y) * x_cos),
            f64::from(trig::sin(x)),
            f64::from(trig::cos(y) * x_cos),
        ]
    }

    fn shape_empty<W: CollisionWorld + ?Sized>(&self, w: &W, p: [i32; 3]) -> bool {
        cell_shape(w, self.data, p, &self.ctx()).is_empty()
    }

    /// Called after every own move with the horizontal distance it covered.
    pub(crate) fn update_auto_jump<W: CollisionWorld + ?Sized>(&mut self, w: &W, xa: f32, za: f32) {
        if !self.can_auto_jump(w) {
            return;
        }
        let begin = self.pos;
        let end = add(begin, [f64::from(xa), 0.0, f64::from(za)]);
        let speed = self.speed();
        let mut diff = [f64::from(xa), 0.0, f64::from(za)];
        let mut dist_sq = (diff[0] * diff[0] + diff[2] * diff[2]) as f32;
        if dist_sq <= 0.001 {
            let (ixa, iza) = (speed * self.move_vec[0], speed * self.move_vec[1]);
            let rad = f64::from(self.yaw * DEG_TO_RAD);
            let (sin, cos) = (trig::sin(rad), trig::cos(rad));
            diff = [
                f64::from(ixa * cos - iza * sin),
                0.0,
                f64::from(iza * cos + ixa * sin),
            ];
            dist_sq = (diff[0] * diff[0] + diff[2] * diff[2]) as f32;
            if dist_sq <= 0.001 {
                return;
            }
        }
        let inv = 1.0 / dist_sq.sqrt();
        let dir = scale(diff, f64::from(inv));
        let facing = self.forward();
        if ((facing[0] * dir[0] + facing[2] * dir[2]) as f32) < -0.15 {
            return;
        }
        let bb = self.bounding_box();
        let mut ceiling = [
            begin[0].floor() as i32,
            bb.max[1].floor() as i32,
            begin[2].floor() as i32,
        ];
        if !self.shape_empty(w, ceiling) {
            return;
        }
        ceiling[1] += 1;
        if !self.shape_empty(w, ceiling) {
            return;
        }
        let mut jump_height = BASE_JUMP_HEIGHT;
        if let Some(a) = self.effects.jump_boost {
            jump_height += (f32::from(a) + 1.0) * 0.75;
        }
        let look_ahead = (speed * LOOK_AHEAD_STEPS).max(1.0 / inv);
        let width = (bb.max[0] - bb.min[0]) as f32;
        let height = (bb.max[1] - bb.min[1]) as f32;
        let seg_end = add(end, scale(dir, f64::from(look_ahead)));
        let test = span(begin, add(seg_end, [0.0, f64::from(height), 0.0]));
        let test = Aabb::new(
            [
                test.min[0] - f64::from(width),
                test.min[1],
                test.min[2] - f64::from(width),
            ],
            [
                test.max[0] + f64::from(width),
                test.max[1],
                test.max[2] + f64::from(width),
            ],
        );
        let lift = [0.0, f64::from(0.51f32), 0.0];
        let (seg_begin, seg_end) = (add(begin, lift), add(seg_end, lift));
        let offset = scale([-dir[2], 0.0, dir[0]], f64::from(width * 0.5));
        let left = span(sub(seg_begin, offset), sub(seg_end, offset));
        let right = span(add(seg_begin, offset), add(seg_end, offset));

        let ctx = self.ctx();
        self.colliders.gather(w, self.data, &test, &ctx);
        let probe = test.inflate(1.0e-7);
        let entity_hit = self
            .entity_colliders
            .iter()
            .find(|e| e.intersects(&probe) && (e.intersects(&left) || e.intersects(&right)));
        let block_hit = || {
            self.colliders
                .boxes
                .iter()
                .find(|b| b.intersects(&left) || b.intersects(&right))
        };
        let Some(obstacle) = entity_hit.or_else(block_hit).copied() else {
            return;
        };
        let mut obstacle_height = obstacle.max[1] as f32;
        let center = obstacle.center();
        let base = center.map(|v| v.floor() as i32);
        let mut steps = 1;
        while (steps as f32) < jump_height {
            let above = [base[0], base[1] + steps, base[2]];
            let shape = cell_shape(w, self.data, above, &ctx);
            if !shape.is_empty() {
                let top = shape.iter().map(|b| b[4]).fold(f64::MIN, f64::max);
                obstacle_height = top as f32 + above[1] as f32;
                if f64::from(obstacle_height) - self.pos[1] > f64::from(jump_height) {
                    return;
                }
            }
            if steps > 1 {
                ceiling[1] += 1;
                if !self.shape_empty(w, ceiling) {
                    return;
                }
            }
            steps += 1;
        }
        let ydelta = (f64::from(obstacle_height) - self.pos[1]) as f32;
        if ydelta > 0.5 && ydelta <= jump_height {
            self.auto_jump_time = 1;
        }
    }
}
