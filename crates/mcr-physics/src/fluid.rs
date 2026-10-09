use crate::data::{FLUID_NONE, FLUID_WATER, StateInfo, flag};
use crate::player::{FluidState, PlayerPhysics};
use crate::world::CollisionWorld;

const HORIZONTAL: [(i32, i32, u8); 4] = [(0, -1, 2), (1, 0, 5), (0, 1, 3), (-1, 0, 4)];

#[derive(Clone, Copy, Default)]
struct Current {
    height: f64,
    acc: [f64; 3],
    count: u32,
}

fn normalize(v: [f64; 3]) -> [f64; 3] {
    let d = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if d < f64::from(1.0e-5f32) {
        [0.0; 3]
    } else {
        [v[0] / d, v[1] / d, v[2] / d]
    }
}

fn blocks_fluid_flow(s: &StateInfo) -> bool {
    s.has_collision() || s.has(flag::SIGN)
}

impl PlayerPhysics {
    fn fluid_height_at<W: CollisionWorld + ?Sized>(
        &self,
        w: &W,
        p: [i32; 3],
        s: &StateInfo,
    ) -> f32 {
        let above = self.state_at(w, [p[0], p[1] + 1, p[2]]);
        if above.fluid == s.fluid {
            1.0
        } else {
            s.own_fluid_height()
        }
    }

    fn solid_face<W: CollisionWorld + ?Sized>(
        &self,
        w: &W,
        p: [i32; 3],
        group: u8,
        face: u8,
    ) -> bool {
        let s = self.state_at(w, p);
        s.fluid != group && !s.has(flag::ICE) && (s.sturdy >> face) & 1 != 0
    }

    fn fluid_flow<W: CollisionWorld + ?Sized>(
        &self,
        w: &W,
        p: [i32; 3],
        st: &StateInfo,
    ) -> [f64; 3] {
        let group = st.fluid;
        let own = st.own_fluid_height();
        let (mut fx, mut fz) = (0.0f64, 0.0f64);
        for (dx, dz, _) in HORIZONTAL {
            let np = [p[0] + dx, p[1], p[2] + dz];
            let ns = self.state_at(w, np);
            if ns.fluid != FLUID_NONE && ns.fluid != group {
                continue;
            }
            let mut nh = ns.own_fluid_height();
            let mut dist = 0.0f32;
            if nh == 0.0 {
                if !blocks_fluid_flow(ns) {
                    let below = self.state_at(w, [np[0], np[1] - 1, np[2]]);
                    if below.fluid == FLUID_NONE || below.fluid == group {
                        nh = below.own_fluid_height();
                        if nh > 0.0 {
                            dist = own - (nh - 0.8888889);
                        }
                    }
                }
            } else if nh > 0.0 {
                dist = own - nh;
            }
            if dist != 0.0 {
                fx += f64::from(dx as f32 * dist);
                fz += f64::from(dz as f32 * dist);
            }
        }
        let mut flow = [fx, 0.0, fz];
        if st.has(flag::FLUID_FALLING) {
            for (dx, dz, face) in HORIZONTAL {
                let np = [p[0] + dx, p[1], p[2] + dz];
                if self.solid_face(w, np, group, face)
                    || self.solid_face(w, [np[0], np[1] + 1, np[2]], group, face)
                {
                    let n = normalize(flow);
                    flow = [n[0], n[1] + -6.0, n[2]];
                    break;
                }
            }
        }
        normalize(flow)
    }

    pub(crate) fn update_fluid_interaction<W: CollisionWorld + ?Sized>(&mut self, w: &W) {
        self.fluid = FluidState::default();
        let ignore_current = self.abilities.flying;
        let bb = self.bounding_box().inflate(-0.001);
        let lo = [
            bb.min[0].floor() as i32,
            bb.min[1].floor() as i32,
            bb.min[2].floor() as i32,
        ];
        let hi = [
            bb.max[0].ceil() as i32 - 1,
            bb.max[1].ceil() as i32 - 1,
            bb.max[2].ceil() as i32 - 1,
        ];
        let mut heights = [0.0f64; 4];
        let mut eyes_water = false;
        let mut cur = [Current::default(); 2];
        let loaded = ((lo[0] - 1) >> 4..=(hi[0] + 1) >> 4).all(|cx| {
            ((lo[2] - 1) >> 4..=(hi[2] + 1) >> 4).all(|cz| w.is_loaded(cx << 4, cz << 4))
        });
        if loaded {
            let entity_y = self.bounding_box().min[1];
            let eye_x = self.pos[0].floor() as i32;
            let eye_z = self.pos[2].floor() as i32;
            let eye_y = self.eye_y();
            for x in lo[0]..=hi[0] {
                for y in lo[1]..=hi[1] {
                    for z in lo[2]..=hi[2] {
                        let p = [x, y, z];
                        let st = self.state_at(w, p);
                        if st.fluid == FLUID_NONE {
                            continue;
                        }
                        let bottom = f64::from(y);
                        let top = bottom + f64::from(self.fluid_height_at(w, p, st));
                        if top < bb.min[1] {
                            continue;
                        }
                        let source = st.has(flag::FLUID_SOURCE);
                        let slot = usize::from(st.fluid - 1) * 2 + usize::from(!source);
                        if x == eye_x && z == eye_z && eye_y >= bottom && st.fluid == FLUID_WATER {
                            let above = self.state_at(w, [x, y + 1, z]);
                            let cam = if source && above.sturdy & 1 != 0 {
                                1.0
                            } else {
                                self.fluid_height_at(w, p, st)
                            };
                            if eye_y <= bottom + f64::from(cam) {
                                eyes_water = true;
                            }
                        }
                        heights[slot] = heights[slot].max(top - entity_y);
                        if !ignore_current {
                            let c = &mut cur[usize::from(st.fluid - 1)];
                            let mut flow = self.fluid_flow(w, p, st);
                            c.height = heights[slot].max(c.height);
                            if c.height < 0.4 {
                                flow = [flow[0] * c.height, flow[1] * c.height, flow[2] * c.height];
                            }
                            c.acc = [c.acc[0] + flow[0], c.acc[1] + flow[1], c.acc[2] + flow[2]];
                            c.count += 1;
                        }
                    }
                }
            }
        }
        self.fluid.water_height = heights[0].max(heights[1]);
        self.fluid.lava_height = heights[2].max(heights[3]);
        self.fluid.eye_in_water = eyes_water;
        let in_water = self.fluid.water_height > 0.0;
        if in_water {
            self.fall_distance = 0.0;
        }
        self.was_touching_water = in_water;
        if !self.abilities.flying {
            if in_water {
                self.apply_current(cur[0], 0.014);
            }
            if self.is_in_lava() {
                let scale = if self.fast_lava {
                    0.007
                } else {
                    0.0023333333333333335
                };
                self.apply_current(cur[1], scale);
            }
        }
    }

    fn apply_current(&mut self, c: Current, scale: f64) {
        let lsq = c.acc[0] * c.acc[0] + c.acc[1] * c.acc[1] + c.acc[2] * c.acc[2];
        if c.count == 0 || lsq < f64::from(1.0e-5f32) {
            return;
        }
        let k = 1.0 / f64::from(c.count);
        let mut imp = [
            c.acc[0] * k * scale,
            c.acc[1] * k * scale,
            c.acc[2] * k * scale,
        ];
        let len = (imp[0] * imp[0] + imp[1] * imp[1] + imp[2] * imp[2]).sqrt();
        if self.vel[0].abs() < 0.003 && self.vel[2].abs() < 0.003 && len < 0.0045000000000000005 {
            let n = normalize(imp);
            imp = [
                n[0] * 0.0045000000000000005,
                n[1] * 0.0045000000000000005,
                n[2] * 0.0045000000000000005,
            ];
        }
        for (v, i) in self.vel.iter_mut().zip(imp) {
            *v += i;
        }
    }

    pub(crate) fn update_swimming<W: CollisionWorld + ?Sized>(&mut self, w: &W) {
        if self.abilities.flying {
            self.swimming = false;
        } else if self.swimming {
            self.swimming = self.sprinting && self.is_in_water();
        } else {
            self.swimming = self.sprinting
                && self.is_under_water()
                && self.state_at(w, self.block_pos()).fluid == FLUID_WATER;
        }
    }
}
