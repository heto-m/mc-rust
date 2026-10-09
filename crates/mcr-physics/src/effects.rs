use crate::aabb::Aabb;
use crate::data::{FLUID_NONE, flag};
use crate::player::PlayerPhysics;
use crate::world::CollisionWorld;

const F_1E5: f64 = 1.0e-5_f32 as f64;
const INSIDE_FLAGS: u32 =
    flag::WEB | flag::SWEET_BERRY | flag::POWDER_SNOW | flag::HONEY | flag::BUBBLE_COLUMN;

impl PlayerPhysics {
    fn make_stuck(&mut self, mult: [f64; 3]) {
        if !self.abilities.flying {
            self.fall_distance = 0.0;
            self.stuck = mult;
        }
    }

    fn honey_old_dy(dy: f64) -> f64 {
        dy / f64::from(0.98f32) + 0.08
    }

    fn honey_new_dy(dy: f64) -> f64 {
        (dy - 0.08) * f64::from(0.98f32)
    }

    fn honey_sliding(&self, p: [i32; 3]) -> bool {
        if self.on_ground || self.pos[1] > f64::from(p[1]) + 0.9375 - 1.0e-7 {
            return false;
        }
        if Self::honey_old_dy(self.vel[1]) >= -0.08 {
            return false;
        }
        let dx = (f64::from(p[0]) + 0.5 - self.pos[0]).abs();
        let dz = (f64::from(p[2]) + 0.5 - self.pos[2]).abs();
        let overlap = 0.4375 + f64::from(0.6f32 / 2.0);
        dx + 1.0e-7 > overlap || dz + 1.0e-7 > overlap
    }

    fn honey_slide(&mut self) {
        let old = Self::honey_old_dy(self.vel[1]);
        if old < -0.13 {
            let k = -0.05 / old;
            self.vel = [self.vel[0] * k, Self::honey_new_dy(-0.05), self.vel[2] * k];
        } else {
            self.vel[1] = Self::honey_new_dy(-0.05);
        }
        self.fall_distance = 0.0;
    }

    /// Whether the player's box swept from `from` to `to` meets the unit cell at `p`.
    fn swept_inside(from: &Aabb, to_delta: [f64; 3], p: [i32; 3]) -> bool {
        let c0 = from.center();
        let c1 = [
            c0[0] + to_delta[0],
            c0[1] + to_delta[1],
            c0[2] + to_delta[2],
        ];
        let mut cell = Aabb::new(
            [f64::from(p[0]), f64::from(p[1]), f64::from(p[2])],
            [
                f64::from(p[0] + 1),
                f64::from(p[1] + 1),
                f64::from(p[2] + 1),
            ],
        );
        for a in 0..3 {
            let grow = (from.max[a] - from.min[a]) * 0.5 - 1.0e-7;
            cell.min[a] -= grow;
            cell.max[a] += grow;
        }
        cell.contains(c1) || cell.contains(c0) || cell.segment_hits(c0, c1)
    }

    pub(crate) fn apply_effects_from_blocks<W: CollisionWorld + ?Sized>(&mut self, w: &W) {
        if self.spectator {
            return;
        }
        if self.on_ground {
            let p = self.on_ground_pos(0.2, w);
            if self.state_at(w, p).has(flag::SLIME) {
                let ay = self.vel[1].abs();
                if ay < 0.1 && !self.keys.sneak {
                    let k = 0.4 + ay * 0.2;
                    self.vel[0] *= k;
                    self.vel[2] *= k;
                }
            }
        }
        let from = self.move_from.unwrap_or(self.pos);
        let to = self.pos;
        let from_box = self.bounding_box_for(self.pose, from);
        let to_box = self.bounding_box().inflate(-F_1E5);
        let from_defl = from_box.inflate(-F_1E5);
        let lo = [
            from_defl.min[0].min(to_box.min[0]).floor() as i32,
            from_defl.min[1].min(to_box.min[1]).floor() as i32,
            from_defl.min[2].min(to_box.min[2]).floor() as i32,
        ];
        let hi = [
            from_defl.max[0].max(to_box.max[0]).floor() as i32,
            from_defl.max[1].max(to_box.max[1]).floor() as i32,
            from_defl.max[2].max(to_box.max[2]).floor() as i32,
        ];
        let delta = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
        for x in lo[0]..=hi[0] {
            for y in lo[1]..=hi[1] {
                for z in lo[2]..=hi[2] {
                    let p = [x, y, z];
                    let st = self.state_at(w, p);
                    if st.flags & INSIDE_FLAGS == 0 || !Self::swept_inside(&from_box, delta, p) {
                        continue;
                    }
                    if st.has(flag::WEB) {
                        self.make_stuck([0.25, f64::from(0.05f32), 0.25]);
                    } else if st.has(flag::SWEET_BERRY) {
                        let k = f64::from(0.8f32);
                        self.make_stuck([k, 0.75, k]);
                    } else if st.has(flag::POWDER_SNOW) {
                        if self.state_at(w, self.block_pos()).has(flag::POWDER_SNOW) {
                            let k = f64::from(0.9f32);
                            self.make_stuck([k, 1.5, k]);
                            self.in_powder_snow = true;
                        }
                    } else if st.has(flag::HONEY) {
                        if self.honey_sliding(p) {
                            self.honey_slide();
                        }
                    } else if st.has(flag::BUBBLE_COLUMN)
                        && !self.abilities.flying
                        && to_box.intersects(&Aabb::new(
                            [f64::from(x), f64::from(y), f64::from(z)],
                            [f64::from(x + 1), f64::from(y + 1), f64::from(z + 1)],
                        ))
                    {
                        let above = self.state_at(w, [x, y + 1, z]);
                        let nothing_above = !above.has_collision() && above.fluid == FLUID_NONE;
                        let drag = st.has(flag::DRAG_DOWN);
                        let vy = self.vel[1];
                        self.vel[1] = match (nothing_above, drag) {
                            (true, true) => (-0.9f64).max(vy - 0.03),
                            (true, false) => 1.8f64.min(vy + 0.1),
                            (false, true) => (-0.3f64).max(vy - 0.03),
                            (false, false) => 0.7f64.min(vy + 0.06),
                        };
                    }
                }
            }
        }
    }
}
