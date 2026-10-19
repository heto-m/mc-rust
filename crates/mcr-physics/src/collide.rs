use crate::aabb::Aabb;
use crate::data::{FLUID_LAVA, PhysicsData, StateInfo, flag};
use crate::world::CollisionWorld;

pub const EPS: f64 = 1.0e-7;
const FALLING_POWDER_SNOW: [f64; 6] = [0.0, 0.0, 0.0, 1.0, 0.8999999761581421, 1.0];
const LAVA_FLOOR: [f64; 6] = [0.0, 0.0, 0.0, 1.0, 0.5, 1.0];
const FULL_CUBE: [f64; 6] = [0.0, 0.0, 0.0, 1.0, 1.0, 1.0];
const UNSTABLE_SCAFFOLDING: [f64; 6] = [0.0, 0.0, 0.0, 1.0, 0.125, 1.0];
/// Stands in for vanilla's infinite border shape; finite so collision arithmetic never sees `inf - inf`.
const BORDER_FAR: f64 = 1.0e9;
const ABOVE_EPS: f64 = 1.0e-5_f32 as f64;

#[derive(Debug, Clone, Copy)]
pub struct CollisionCtx {
    pub y: f64,
    pub fall_distance: f64,
    pub powder_walk: bool,
    pub descending: bool,
    pub stand_on_lava: bool,
}

impl Default for CollisionCtx {
    fn default() -> Self {
        Self {
            y: 0.0,
            fall_distance: 0.0,
            powder_walk: false,
            descending: false,
            stand_on_lava: false,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ShapeRef {
    pub start: u32,
    pub len: u8,
    pub full: bool,
}

#[derive(Debug, Default, Clone)]
pub struct Colliders {
    pub boxes: Vec<Aabb>,
    pub shapes: Vec<ShapeRef>,
}

fn effective<'a>(
    data: &'a PhysicsData,
    s: &'a StateInfo,
    pos: [i32; 3],
    ctx: &CollisionCtx,
) -> &'a [[f64; 6]] {
    if s.flags & (flag::POWDER_SNOW | flag::SCAFFOLDING) == 0 {
        return data.boxes(s);
    }
    let above = ctx.y > f64::from(pos[1]) + 1.0 - ABOVE_EPS;
    if s.has(flag::POWDER_SNOW) {
        if ctx.fall_distance > 2.5 {
            std::slice::from_ref(&FALLING_POWDER_SNOW)
        } else if ctx.powder_walk && above && !ctx.descending {
            std::slice::from_ref(&FULL_CUBE)
        } else {
            &[]
        }
    } else if above && !ctx.descending {
        data.boxes(s)
    } else if s.has(flag::SCAFFOLDING_UNSTABLE) && ctx.y > f64::from(pos[1]) - ABOVE_EPS {
        std::slice::from_ref(&UNSTABLE_SCAFFOLDING)
    } else {
        &[]
    }
}

/// The world border's square, as the local player collides with it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BorderBounds {
    pub min_x: f64,
    pub min_z: f64,
    pub max_x: f64,
    pub max_z: f64,
}

impl BorderBounds {
    /// Bounds from a centre and side length; non-finite input yields `None`.
    pub fn new(center_x: f64, center_z: f64, size: f64, absolute_max: f64) -> Option<Self> {
        if !(center_x.is_finite() && center_z.is_finite() && size.is_finite()) {
            return None;
        }
        let lim = absolute_max.abs();
        let half = size.max(0.0) / 2.0;
        Some(Self {
            min_x: (center_x - half).clamp(-lim, lim),
            min_z: (center_z - half).clamp(-lim, lim),
            max_x: (center_x + half).clamp(-lim, lim),
            max_z: (center_z + half).clamp(-lim, lim),
        })
    }

    fn distance(&self, x: f64, z: f64) -> f64 {
        (x - self.min_x)
            .min(self.max_x - x)
            .min(z - self.min_z)
            .min(self.max_z - z)
    }

    fn within(&self, x: f64, z: f64, margin: f64) -> bool {
        x >= self.min_x - margin
            && x < self.max_x + margin
            && z >= self.min_z - margin
            && z < self.max_z + margin
    }

    /// Vanilla `isInsideCloseToBorder` for an entity at `pos` testing `region`.
    pub fn is_close(&self, pos: [f64; 3], region: &Aabb) -> bool {
        let xs = region.max[0] - region.min[0];
        let zs = region.max[2] - region.min[2];
        let size = if xs.abs() >= zs.abs() { xs } else { zs };
        let margin = size.abs().max(1.0);
        self.distance(pos[0], pos[2]) < margin * 2.0 && self.within(pos[0], pos[2], margin)
    }

    /// Everything outside the block-aligned square, as four slabs.
    pub fn boxes(&self) -> [Aabb; 4] {
        let (x0, x1) = (self.min_x.floor(), self.max_x.ceil());
        let (z0, z1) = (self.min_z.floor(), self.max_z.ceil());
        let f = BORDER_FAR;
        [
            Aabb::new([-f, -f, -f], [x0, f, f]),
            Aabb::new([x1, -f, -f], [f, f, f]),
            Aabb::new([x0, -f, -f], [x1, f, z0]),
            Aabb::new([x0, -f, z1], [x1, f, f]),
        ]
    }
}

fn is_full_cube(b: &[[f64; 6]]) -> bool {
    b.len() == 1 && b[0] == FULL_CUBE
}

fn shape_hits(boxes: &[Aabb], full: bool, region: &Aabb) -> bool {
    if full {
        return region.intersects(&boxes[0]);
    }
    boxes
        .iter()
        .any(|b| (0..3).all(|a| b.max[a].min(region.max[a]) - b.min[a].max(region.min[a]) > EPS))
}

fn floor(x: f64) -> i32 {
    x.floor() as i32
}

fn visit_cells<W: CollisionWorld + ?Sized>(
    world: &W,
    data: &PhysicsData,
    region: &Aabb,
    ctx: &CollisionCtx,
    mut f: impl FnMut([i32; 3], &[Aabb], bool) -> bool,
) {
    let lo = [
        floor(region.min[0] - EPS) - 1,
        floor(region.min[1] - EPS) - 1,
        floor(region.min[2] - EPS) - 1,
    ];
    let hi = [
        floor(region.max[0] + EPS) + 1,
        floor(region.max[1] + EPS) + 1,
        floor(region.max[2] + EPS) + 1,
    ];
    let mut tmp: [Aabb; 8] = [Aabb::new([0.0; 3], [0.0; 3]); 8];
    for z in lo[2]..=hi[2] {
        for y in lo[1]..=hi[1] {
            for x in lo[0]..=hi[0] {
                let p = [x, y, z];
                let mut border = 0;
                let mut relevant = true;
                for a in 0..3 {
                    if p[a] == lo[a] {
                        border += 1;
                        relevant &= data.overhang_hi(a);
                    } else if p[a] == hi[a] {
                        border += 1;
                        relevant &= data.overhang_lo(a);
                    }
                }
                if border >= 2 || !relevant {
                    continue;
                }
                let s = data.state(world.block_state(x, y, z));
                if border == 1 && !s.has(flag::LARGE) {
                    continue;
                }
                let raw = shape_of(world, data, s, p, ctx);
                if raw.is_empty() || raw.len() > tmp.len() {
                    continue;
                }
                let off = [f64::from(x), f64::from(y), f64::from(z)];
                for (i, b) in raw.iter().enumerate() {
                    tmp[i] = Aabb::new(
                        [b[0] + off[0], b[1] + off[1], b[2] + off[2]],
                        [b[3] + off[0], b[4] + off[1], b[5] + off[2]],
                    );
                }
                let full = is_full_cube(raw);
                if shape_hits(&tmp[..raw.len()], full, region) && f(p, &tmp[..raw.len()], full) {
                    return;
                }
            }
        }
    }
}

/// The collision shape of the cell at `p` in the entity's context, as `getCollisionShape`.
pub fn cell_shape<'a, W: CollisionWorld + ?Sized>(
    world: &W,
    data: &'a PhysicsData,
    p: [i32; 3],
    ctx: &CollisionCtx,
) -> &'a [[f64; 6]] {
    shape_of(
        world,
        data,
        data.state(world.block_state(p[0], p[1], p[2])),
        p,
        ctx,
    )
}

fn shape_of<'a, W: CollisionWorld + ?Sized>(
    world: &W,
    data: &'a PhysicsData,
    s: &'a StateInfo,
    p: [i32; 3],
    ctx: &CollisionCtx,
) -> &'a [[f64; 6]] {
    if s.box_len != 0 && s.flags & (flag::POWDER_SNOW | flag::SCAFFOLDING) == 0 {
        return data.boxes(s);
    }
    let lava_floor = ctx.stand_on_lava
        && s.box_len == 0
        && s.fluid == FLUID_LAVA
        && s.has(flag::FLUID_SOURCE)
        && ctx.y > f64::from(p[1]) + 0.5 - ABOVE_EPS
        && data.state(world.block_state(p[0], p[1] + 1, p[2])).fluid != FLUID_LAVA;
    if lava_floor {
        std::slice::from_ref(&LAVA_FLOOR)
    } else if s.box_len == 0 && !s.has(flag::POWDER_SNOW | flag::SCAFFOLDING) {
        &[]
    } else {
        effective(data, s, p, ctx)
    }
}

pub fn any_collision<W: CollisionWorld + ?Sized>(
    world: &W,
    data: &PhysicsData,
    region: &Aabb,
    ctx: &CollisionCtx,
) -> bool {
    let mut hit = false;
    visit_cells(world, data, region, ctx, |_, _, _| {
        hit = true;
        true
    });
    hit
}

/// Vanilla `collidesWithSuffocatingBlock` for a region inside one block column; suffocating
/// shapes never overhang their cell, so only the cells the region covers are read.
pub fn any_suffocating<W: CollisionWorld + ?Sized>(
    world: &W,
    data: &PhysicsData,
    region: &Aabb,
) -> bool {
    let lo = region.min.map(floor);
    let hi = region.max.map(floor);
    let mut tmp: [Aabb; 8] = [Aabb::new([0.0; 3], [0.0; 3]); 8];
    for x in lo[0]..=hi[0] {
        for z in lo[2]..=hi[2] {
            for y in lo[1]..=hi[1] {
                let s = data.state(world.block_state(x, y, z));
                if !s.has(flag::SUFFOCATING) {
                    continue;
                }
                let raw = data.boxes(s);
                if raw.is_empty() || raw.len() > tmp.len() {
                    continue;
                }
                let off = [f64::from(x), f64::from(y), f64::from(z)];
                for (i, b) in raw.iter().enumerate() {
                    tmp[i] = Aabb::new(
                        [b[0] + off[0], b[1] + off[1], b[2] + off[2]],
                        [b[3] + off[0], b[4] + off[1], b[5] + off[2]],
                    );
                }
                if shape_hits(&tmp[..raw.len()], is_full_cube(raw), region) {
                    return true;
                }
            }
        }
    }
    false
}

/// Returns the supporting block under `bb`, tie-broken like vanilla's `BlockPos.compareTo`.
pub fn find_supporting_block<W: CollisionWorld + ?Sized>(
    world: &W,
    data: &PhysicsData,
    test: &Aabb,
    entity_pos: [f64; 3],
    ctx: &CollisionCtx,
) -> Option<[i32; 3]> {
    let mut best: Option<[i32; 3]> = None;
    let mut best_d = f64::MAX;
    visit_cells(world, data, test, ctx, |p, _, _| {
        let dx = f64::from(p[0]) + 0.5 - entity_pos[0];
        let dy = f64::from(p[1]) + 0.5 - entity_pos[1];
        let dz = f64::from(p[2]) + 0.5 - entity_pos[2];
        let d = dx * dx + dy * dy + dz * dz;
        let greater = |b: [i32; 3]| (b[1], b[2], b[0]) < (p[1], p[2], p[0]);
        if d < best_d || (d == best_d && best.is_none_or(greater)) {
            best = Some(p);
            best_d = d;
        }
        false
    });
    best
}

impl Colliders {
    pub fn gather<W: CollisionWorld + ?Sized>(
        &mut self,
        world: &W,
        data: &PhysicsData,
        region: &Aabb,
        ctx: &CollisionCtx,
    ) {
        self.boxes.clear();
        self.shapes.clear();
        let (boxes, shapes) = (&mut self.boxes, &mut self.shapes);
        visit_cells(world, data, region, ctx, |_, bs, full| {
            shapes.push(ShapeRef {
                start: boxes.len() as u32,
                len: bs.len() as u8,
                full,
            });
            boxes.extend_from_slice(bs);
            false
        });
    }

    /// Adds one box shape (an entity or a border slab) gathered outside the block scan.
    pub fn add_box(&mut self, b: Aabb) {
        self.shapes.push(ShapeRef {
            start: self.boxes.len() as u32,
            len: 1,
            full: false,
        });
        self.boxes.push(b);
    }

    fn shape_boxes(&self, s: &ShapeRef) -> &[Aabb] {
        &self.boxes[s.start as usize..s.start as usize + s.len as usize]
    }

    pub fn shape_collide(&self, s: &ShapeRef, axis: usize, moving: &Aabb, distance: f64) -> f64 {
        if distance.abs() < EPS {
            return 0.0;
        }
        let (b, c) = ((axis + 1) % 3, (axis + 2) % 3);
        let mut best = distance;
        for bx in self.shape_boxes(s) {
            let overlap = [b, c]
                .iter()
                .all(|&o| bx.max[o] > moving.min[o] + EPS && bx.min[o] <= moving.max[o] - EPS);
            if !overlap {
                continue;
            }
            if distance > 0.0 {
                if bx.min[axis] > moving.max[axis] - EPS {
                    best = best.min(bx.min[axis] - moving.max[axis]);
                }
            } else if bx.max[axis] <= moving.min[axis] + EPS {
                best = best.max(bx.max[axis] - moving.min[axis]);
            }
        }
        best
    }

    pub fn collide_axis(&self, axis: usize, moving: &Aabb, mut distance: f64) -> f64 {
        for s in &self.shapes {
            if distance.abs() < EPS {
                return 0.0;
            }
            distance = self.shape_collide(s, axis, moving, distance);
        }
        distance
    }

    pub fn collide_with_shapes(&self, movement: [f64; 3], bb: &Aabb) -> [f64; 3] {
        if self.shapes.is_empty() {
            return movement;
        }
        let order = if movement[0].abs() < movement[2].abs() {
            [1, 2, 0]
        } else {
            [1, 0, 2]
        };
        let mut resolved = [0.0; 3];
        for axis in order {
            if movement[axis] != 0.0 {
                resolved[axis] = self.collide_axis(axis, &bb.move_by(resolved), movement[axis]);
            }
        }
        resolved
    }

    /// Distinct Y edges of every shape touching `region`, relative to `base_y`, in `(skip, max]`.
    pub fn step_candidates(
        &self,
        region: &Aabb,
        base_y: f64,
        max_step: f32,
        skip: f32,
        out: &mut Vec<f32>,
    ) {
        out.clear();
        let mut ys: [f64; 16] = [0.0; 16];
        for s in &self.shapes {
            let boxes = self.shape_boxes(s);
            if !shape_hits(boxes, s.full, region) {
                continue;
            }
            let mut n = 0;
            for b in boxes {
                for y in [b.min[1], b.max[1]] {
                    if n < ys.len() && !ys[..n].contains(&y) {
                        ys[n] = y;
                        n += 1;
                    }
                }
            }
            ys[..n].sort_by(|a, b| a.total_cmp(b));
            for &coord in &ys[..n] {
                let rel = (coord - base_y) as f32;
                if rel < 0.0 || rel == skip {
                    continue;
                }
                if rel > max_step {
                    break;
                }
                if !out.contains(&rel) {
                    out.push(rel);
                }
            }
        }
        out.sort_by(|a, b| a.total_cmp(b));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct One;
    impl CollisionWorld for One {
        fn block_state(&self, _x: i32, y: i32, _z: i32) -> u32 {
            if y < 0 {
                let v = mcr_data::v26_3();
                v.block_info(v.block_by_name("minecraft:stone").unwrap())
                    .unwrap()
                    .default_state
            } else {
                0
            }
        }
    }

    #[test]
    fn falls_onto_floor() {
        let data = PhysicsData::v26_3();
        let bb = Aabb::new([0.2, 0.5, 0.2], [0.8, 2.3, 0.8]);
        let mut c = Colliders::default();
        c.gather(
            &One,
            data,
            &bb.expand_towards([0.0, -2.0, 0.0]),
            &CollisionCtx::default(),
        );
        let r = c.collide_with_shapes([0.0, -2.0, 0.0], &bb);
        assert_eq!(r, [0.0, -0.5, 0.0]);
    }
}
