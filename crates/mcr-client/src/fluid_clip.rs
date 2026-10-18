//! `Item#getPlayerPOVHitResult`: the eye ray against outline shapes and, by mode, fluids. Buckets
//! and boats decide their use result from it.

use mcr_buf::BlockPos;

use crate::adventure::AdventureCheck;
use crate::state::ClientState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClipFluid {
    None,
    SourceOnly,
    Any,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ClipHit {
    pub pos: BlockPos,
    pub state: u32,
    pub fluid: bool,
}

fn ray_box(origin: [f64; 3], dir: [f64; 3], min: [f64; 3], max: [f64; 3]) -> Option<f64> {
    let (mut t0, mut t1) = (f64::NEG_INFINITY, f64::INFINITY);
    for a in 0..3 {
        if dir[a].abs() < 1.0e-12 {
            if origin[a] < min[a] || origin[a] > max[a] {
                return None;
            }
            continue;
        }
        let (mut lo, mut hi) = ((min[a] - origin[a]) / dir[a], (max[a] - origin[a]) / dir[a]);
        if lo > hi {
            std::mem::swap(&mut lo, &mut hi);
        }
        t0 = t0.max(lo);
        t1 = t1.min(hi);
        if t0 > t1 {
            return None;
        }
    }
    (t1 >= 0.0).then_some(t0.max(0.0))
}

fn short(name: &str) -> &str {
    name.strip_prefix("minecraft:").unwrap_or(name)
}

impl ClientState {
    fn view_ray(&self) -> ([f64; 3], [f64; 3]) {
        let p = &self.player.physics;
        let eye = [p.pos[0], p.pos[1] + f64::from(p.eye_height()), p.pos[2]];
        let (yaw, pitch) = (
            f64::from(p.yaw).to_radians(),
            f64::from(p.pitch).to_radians(),
        );
        let dir = [
            -yaw.sin() * pitch.cos(),
            -pitch.sin(),
            yaw.cos() * pitch.cos(),
        ];
        (eye, dir)
    }

    /// Fluid surface height in its cell, or `None` when `mode` does not pick it.
    fn fluid_height(&self, state: u32, pos: [i32; 3], mode: ClipFluid) -> Option<f64> {
        let f = self.data.block_state(state)?.fluid?;
        if mode == ClipFluid::None || (mode == ClipFluid::SourceOnly && !f.source) {
            return None;
        }
        let above = self
            .chunks
            .block(pos[0], pos[1] + 1, pos[2])
            .and_then(|b| self.data.block_state(b.0))
            .and_then(|i| i.fluid);
        let full = f.falling || above.is_some_and(|a| a.fluid == f.fluid);
        Some(if full { 1.0 } else { f64::from(f.amount) / 9.0 })
    }

    pub(crate) fn clip_from_eye(&self, mode: ClipFluid, max: f64) -> Option<ClipHit> {
        let (origin, dir) = self.view_ray();
        let max = if max.is_finite() {
            max.clamp(0.0, 64.0)
        } else {
            0.0
        };
        let mut cell = origin.map(|v| v.floor() as i32);
        let mut step = [0i32; 3];
        let mut t_next = [f64::INFINITY; 3];
        let mut t_delta = [f64::INFINITY; 3];
        for a in 0..3 {
            if dir[a] > 0.0 {
                step[a] = 1;
                t_delta[a] = 1.0 / dir[a];
                t_next[a] = (f64::from(cell[a]) + 1.0 - origin[a]) / dir[a];
            } else if dir[a] < 0.0 {
                step[a] = -1;
                t_delta[a] = -1.0 / dir[a];
                t_next[a] = (origin[a] - f64::from(cell[a])) / -dir[a];
            }
        }
        let limit = (max * 3.0).ceil() as usize + 4;
        for _ in 0..limit {
            if let Some(b) = self.chunks.block(cell[0], cell[1], cell[2]) {
                let o = cell.map(f64::from);
                let mut best: Option<(f64, bool)> = None;
                for s in self.data.outline_shape(b.0) {
                    let lo = std::array::from_fn(|a| o[a] + f64::from(s.min[a]));
                    let hi = std::array::from_fn(|a| o[a] + f64::from(s.max[a]));
                    if let Some(t) = ray_box(origin, dir, lo, hi)
                        && t <= max
                        && best.is_none_or(|(bt, _)| t < bt)
                    {
                        best = Some((t, false));
                    }
                }
                if let Some(h) = self.fluid_height(b.0, cell, mode) {
                    let hi = [o[0] + 1.0, o[1] + h, o[2] + 1.0];
                    if let Some(t) = ray_box(origin, dir, o, hi)
                        && t <= max
                        && best.is_none_or(|(bt, _)| t < bt)
                    {
                        best = Some((t, true));
                    }
                }
                if let Some((_, fluid)) = best {
                    return Some(ClipHit {
                        pos: BlockPos::new(cell[0], cell[1], cell[2]),
                        state: b.0,
                        fluid,
                    });
                }
            }
            let axis = (0..3)
                .min_by(|&a, &b| t_next[a].total_cmp(&t_next[b]))
                .unwrap_or(0);
            if t_next[axis] > max {
                return None;
            }
            cell[axis] += step[axis];
            t_next[axis] += t_delta[axis];
        }
        None
    }

    /// Whether `BucketItem#use` or `BoatItem#use` of `item` succeeds (and swings) where the player looks.
    pub(crate) fn fluid_item_succeeds(&self, item: &str) -> bool {
        let item = short(item);
        let range = self.block_interaction_range();
        if item.ends_with("_boat") || item.ends_with("_raft") {
            return self.clip_from_eye(ClipFluid::Any, range).is_some();
        }
        if item != "bucket" {
            return self.clip_from_eye(ClipFluid::None, range).is_some();
        }
        let Some(hit) = self.clip_from_eye(ClipFluid::SourceOnly, range) else {
            return false;
        };
        if !self.may_build()
            && !self.adventure_allows(self.held_stack(), AdventureCheck::CanPlaceOn, hit.pos)
        {
            return false;
        }
        let Some(info) = self.data.block_state(hit.state) else {
            return false;
        };
        let name = self.data.block_name(info.block).map_or("", short);
        name == "powder_snow" || info.fluid.is_some_and(|f| f.source)
    }
}
