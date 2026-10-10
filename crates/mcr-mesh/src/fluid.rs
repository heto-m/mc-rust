//! Water and lava surfaces, following vanilla's fluid renderer.

use mcr_render::{Face, ModelQuadDesc, SectionMesh, model_ao, pack_light};

use crate::cache::{
    ATTR_HALF_TRANSPARENT, ATTR_SOLID, FluidKind, MASK_EMPTY, MASK_FULL, StateModel,
};
use crate::mesher::{Ctx, Mesher};
use crate::tint::{TINT_NONE, TINT_WATER};

const UP: [i32; 3] = [0, 1, 0];
const DOWN: [i32; 3] = [0, -1, 0];
const MAX_FLUID_HEIGHT: f32 = 0.888_888_9;
const INSET: f32 = 0.001;

fn add(a: [i32; 3], b: [i32; 3]) -> [i32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn kind_at(ctx: &Ctx, c: [i32; 3]) -> FluidKind {
    ctx.cache.state(ctx.state(c)).fluid.kind
}

fn height_at(ctx: &Ctx, kind: FluidKind, c: [i32; 3]) -> f32 {
    let state = ctx.cache.state(ctx.state(c));
    if state.fluid.kind == kind {
        if kind_at(ctx, add(c, UP)) == kind {
            1.0
        } else {
            f32::from(state.fluid.amount) / 9.0
        }
    } else if ctx.attr(c) & ATTR_SOLID == 0 {
        0.0
    } else {
        -1.0
    }
}

fn own_height(ctx: &Ctx, c: [i32; 3]) -> f32 {
    f32::from(ctx.cache.state(ctx.state(c)).fluid.amount) / 9.0
}

fn weighted(acc: &mut [f32; 2], h: f32) {
    if h >= 0.8 {
        acc[0] += h * 10.0;
        acc[1] += 10.0;
    } else if h >= 0.0 {
        acc[0] += h;
        acc[1] += 1.0;
    }
}

pub(crate) fn corner_height(
    ctx: &Ctx,
    kind: FluidKind,
    own: f32,
    a: f32,
    b: f32,
    corner: [i32; 3],
) -> f32 {
    average_height(own, a, b, || height_at(ctx, kind, corner))
}

fn average_height(own: f32, a: f32, b: f32, corner: impl FnOnce() -> f32) -> f32 {
    if a >= 1.0 || b >= 1.0 {
        return 1.0;
    }
    let mut acc = [0.0; 2];
    if a > 0.0 || b > 0.0 {
        let h = corner();
        if h >= 1.0 {
            return 1.0;
        }
        weighted(&mut acc, h);
    }
    weighted(&mut acc, own);
    weighted(&mut acc, a);
    weighted(&mut acc, b);
    acc[0] / acc[1]
}

/// Vanilla `FlowingFluid.getFlow` horizontal part; only its direction matters for texturing.
pub(crate) fn flow_vector(ctx: &Ctx, kind: FluidKind, cell: [i32; 3]) -> [f32; 2] {
    let own = own_height(ctx, cell);
    let mut flow = [0.0f32; 2];
    for (dx, dz) in [(0, -1), (0, 1), (-1, 0), (1, 0)] {
        let n = add(cell, [dx, 0, dz]);
        let nk = kind_at(ctx, n);
        if nk != FluidKind::None && nk != kind {
            continue;
        }
        let mut height = if nk == kind { own_height(ctx, n) } else { 0.0 };
        let mut distance = 0.0;
        if height == 0.0 {
            if ctx.attr(n) & ATTR_SOLID == 0 {
                let below = add(n, DOWN);
                let bk = kind_at(ctx, below);
                if bk == FluidKind::None || bk == kind {
                    height = if bk == kind {
                        own_height(ctx, below)
                    } else {
                        0.0
                    };
                    if height > 0.0 {
                        distance = own - (height - MAX_FLUID_HEIGHT);
                    }
                }
            }
        } else if height > 0.0 {
            distance = own - height;
        }
        if distance != 0.0 {
            flow[0] += dx as f32 * distance;
            flow[1] += dz as f32 * distance;
        }
    }
    let len = (flow[0] * flow[0] + flow[1] * flow[1]).sqrt();
    if len > 0.0 {
        [flow[0] / len, flow[1] / len]
    } else {
        [0.0; 2]
    }
}

/// UVs of the top face corners (nw, sw, se, ne) for a flow direction, in sprite fractions.
pub(crate) fn top_uvs(flow: [f32; 2]) -> [[f32; 2]; 4] {
    if flow == [0.0; 2] {
        return [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]];
    }
    let angle = flow[1].atan2(flow[0]) - std::f32::consts::FRAC_PI_2;
    let s = angle.sin() * 0.25;
    let c = angle.cos() * 0.25;
    [
        [0.5 + (-c - s), 0.5 + (-c + s)],
        [0.5 + (-c + s), 0.5 + (c + s)],
        [0.5 + (c + s), 0.5 + (c - s)],
        [0.5 + (c - s), 0.5 + (-c - s)],
    ]
}

/// Whether the face `dir` of a fluid box of `height` is hidden by the occlusion mask `mask` of the
/// neighbour (vanilla `isFaceOccludedByState`).
fn occluded(ctx: &Ctx, face: usize, height: f32, neighbor: [i32; 3]) -> bool {
    let mask = ctx.cache.face_mask(ctx.state(neighbor), face ^ 1);
    match mask {
        MASK_EMPTY => false,
        MASK_FULL => face != 2 || height >= 1.0,
        _ if face == 2 || face == 3 => false,
        _ => {
            let grid = ctx.cache.mask(mask);
            let rows = (0..16usize).filter(|&v| (v as f32 + 0.5) / 16.0 < height);
            rows.into_iter()
                .all(|v| (0..16usize).all(|u| grid[v / 4] >> ((v % 4) * 16 + u) & 1 != 0))
        }
    }
}

fn max_light(a: (u8, u8), b: (u8, u8)) -> u16 {
    pack_light(a.0.max(b.0) * 16, a.1.max(b.1) * 16)
}

/// Emits the fluid at `cell`; returns whether any quad uses a biome tint.
pub(crate) fn mesh_fluid(
    mesher: &mut Mesher,
    ctx: &Ctx,
    cell: [i32; 3],
    id: u16,
    sm: &StateModel,
    out: &mut SectionMesh,
) -> bool {
    let kind = sm.fluid.kind;
    let sprites = ctx.cache.fluid_sprites;
    let (still, flow_sprite) = match kind {
        FluidKind::Water => (sprites.water_still, sprites.water_flow),
        _ => (sprites.lava_still, sprites.lava_flow),
    };
    let translucent = kind == FluidKind::Water;
    let tint = if translucent { TINT_WATER } else { TINT_NONE };
    let neighbor = |d: [i32; 3]| add(cell, d);
    let up_cell = neighbor(UP);
    let down_cell = neighbor(DOWN);

    let side_dirs: [([i32; 3], usize); 4] = [
        ([0, 0, -1], 5),
        ([0, 0, 1], 4),
        ([-1, 0, 0], 1),
        ([1, 0, 0], 0),
    ];
    let same = |c: [i32; 3]| kind_at(ctx, c) == kind;
    let self_occluded = |face: usize| ctx.cache.face_mask(id, face) == MASK_FULL;

    let render_up = !same(up_cell);
    let render_down =
        !same(down_cell) && !self_occluded(3) && !occluded(ctx, 3, MAX_FLUID_HEIGHT, down_cell);
    let render_side: [bool; 4] =
        std::array::from_fn(|i| !same(neighbor(side_dirs[i].0)) && !self_occluded(side_dirs[i].1));
    if !(render_up || render_down || render_side.iter().any(|&b| b)) {
        return false;
    }

    let own = height_at(ctx, kind, cell);
    let h = |d: [i32; 3]| height_at(ctx, kind, neighbor(d));
    let (mut nw, mut sw, mut se, mut ne) = (1.0f32, 1.0f32, 1.0f32, 1.0f32);
    if own < 1.0 {
        let (n, s, e, w) = (h([0, 0, -1]), h([0, 0, 1]), h([1, 0, 0]), h([-1, 0, 0]));
        let corner = |dx, dz| neighbor([dx, 0, dz]);
        ne = corner_height(ctx, kind, own, n, e, corner(1, -1));
        nw = corner_height(ctx, kind, own, n, w, corner(-1, -1));
        se = corner_height(ctx, kind, own, s, e, corner(1, 1));
        sw = corner_height(ctx, kind, own, s, w, corner(-1, 1));
    }

    let [cx, cy, cz] = cell.map(|v| v as f32);
    let here = ctx.light(cell);
    let light_here_up = max_light(here, ctx.light(up_cell));
    let light_down = max_light(ctx.light(down_cell), here);
    let cell_u8 = [cell[0] as u8, cell[1] as u8, cell[2] as u8];
    mesher.note_tint(cell);
    let mut emit = |corners: [[f32; 3]; 4],
                    uvs: [[f32; 2]; 4],
                    layer: u16,
                    light: u16,
                    shade: Face,
                    back: bool| {
        // Vertex order is vanilla's cycle; slots put its v0-v2 diagonal on the shader's shared edge.
        let slot_of = [1usize, 3, 2, 0];
        let mut build = |order: [usize; 4]| {
            let mut c = [[0.0; 3]; 4];
            let mut t = [[0.0; 2]; 4];
            for (i, &v) in order.iter().enumerate() {
                c[slot_of[i]] = corners[v];
                t[slot_of[i]] = uvs[v];
            }
            mesher.push_fluid_quad(
                &ModelQuadDesc {
                    corners: c,
                    uvs: t,
                    layer,
                    light: [light; 4],
                    shade_face: shade,
                    flags: 0,
                    ao: [model_ao(1.0); 4],
                    tint,
                    cell: cell_u8,
                },
                translucent,
                out,
            );
        };
        build([0, 1, 2, 3]);
        if back {
            build([0, 3, 2, 1]);
        }
    };

    if render_up {
        let min = nw.min(sw).min(se).min(ne);
        let blocked = min >= 1.0 && ctx.cache.face_mask(ctx.state(up_cell), 3) == MASK_FULL;
        if !blocked {
            let lower = |v: f32| v - INSET;
            let (a, b, c, d) = (nw, sw, se, ne);
            nw = lower(a);
            sw = lower(b);
            se = lower(c);
            ne = lower(d);
            let uvs = if flow_vector(ctx, kind, cell) == [0.0; 2] {
                top_uvs([0.0; 2])
            } else {
                top_uvs(flow_vector(ctx, kind, cell))
            };
            let moving = uvs != top_uvs([0.0; 2]);
            let layer = if moving { flow_sprite } else { still };
            let backward = (-1..=1).any(|dx| {
                (-1..=1).any(|dz| {
                    let p = add(up_cell, [dx, 0, dz]);
                    kind_at(ctx, p) != kind && ctx.attr(p) & crate::cache::ATTR_SOLID_RENDER == 0
                })
            });
            emit(
                [
                    [cx, cy + nw, cz],
                    [cx, cy + sw, cz + 1.0],
                    [cx + 1.0, cy + se, cz + 1.0],
                    [cx + 1.0, cy + ne, cz],
                ],
                uvs,
                layer,
                light_here_up,
                Face::PosY,
                backward,
            );
        }
    }
    let bottom = if render_down { INSET } else { 0.0 };
    if render_down {
        emit(
            [
                [cx, cy + bottom, cz],
                [cx + 1.0, cy + bottom, cz],
                [cx + 1.0, cy + bottom, cz + 1.0],
                [cx, cy + bottom, cz + 1.0],
            ],
            [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            still,
            light_down,
            Face::NegY,
            false,
        );
    }

    for (i, (dir, face)) in side_dirs.iter().enumerate() {
        if !render_side[i] {
            continue;
        }
        let (h0, h1, x0, z0, x1, z1) = match i {
            0 => (nw, ne, cx, cz + INSET, cx + 1.0, cz + INSET),
            1 => (se, sw, cx + 1.0, cz + 1.0 - INSET, cx, cz + 1.0 - INSET),
            2 => (sw, nw, cx + INSET, cz + 1.0, cx + INSET, cz),
            _ => (ne, se, cx + 1.0 - INSET, cz, cx + 1.0 - INSET, cz + 1.0),
        };
        let ncell = neighbor(*dir);
        if occluded(ctx, *face, h0.max(h1), ncell) {
            continue;
        }
        let overlay = translucent && ctx.attr(ncell) & ATTR_HALF_TRANSPARENT != 0;
        let layer = if overlay {
            sprites.water_overlay
        } else {
            flow_sprite
        };
        emit(
            [
                [x0, cy + h0, z0],
                [x1, cy + h1, z1],
                [x1, cy + bottom, z1],
                [x0, cy + bottom, z0],
            ],
            [
                [0.0, (1.0 - h0) * 0.5],
                [0.5, (1.0 - h1) * 0.5],
                [0.5, 0.5],
                [0.0, 0.5],
            ],
            layer,
            light_here_up,
            Face::ALL[*face],
            !overlay,
        );
    }
    translucent
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: f32 = 8.0 / 9.0;

    fn avg(own: f32, a: f32, b: f32, corner: f32) -> f32 {
        average_height(own, a, b, || corner)
    }

    #[test]
    fn lone_source_corner_averages_with_air() {
        let want = SOURCE * 10.0 / 12.0;
        assert!((avg(SOURCE, 0.0, 0.0, 0.0) - want).abs() < 1e-6);
    }

    #[test]
    fn source_next_to_source_pulls_the_corner_up() {
        let got = avg(SOURCE, SOURCE, 0.0, 0.0);
        assert!((got - SOURCE * 20.0 / 22.0).abs() < 1e-6);
    }

    #[test]
    fn low_levels_weigh_one_each() {
        let got = avg(2.0 / 9.0, 1.0 / 9.0, 0.0, 0.0);
        assert!((got - 3.0 / 9.0 / 4.0).abs() < 1e-6);
    }

    #[test]
    fn fluid_above_a_neighbour_or_the_corner_makes_the_corner_full() {
        assert_eq!(avg(0.5, 1.0, 0.0, 0.0), 1.0);
        assert_eq!(avg(0.5, 0.0, 1.0, 0.0), 1.0);
        assert_eq!(avg(0.5, 0.3, 0.0, 1.0), 1.0);
    }

    #[test]
    fn solid_neighbours_are_ignored() {
        let got = avg(SOURCE, -1.0, -1.0, -1.0);
        assert!((got - SOURCE).abs() < 1e-6);
    }

    #[test]
    fn corner_is_only_sampled_beside_fluid() {
        let got = average_height(0.5, 0.0, 0.0, || panic!("corner sampled"));
        assert!((got - 0.5 / 3.0).abs() < 1e-6);
    }

    #[test]
    fn still_water_uses_unrotated_uvs() {
        assert_eq!(
            top_uvs([0.0; 2]),
            [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]]
        );
    }

    #[test]
    fn flow_uvs_rotate_with_the_flow_direction() {
        let east = top_uvs([1.0, 0.0]);
        let south = top_uvs([0.0, 1.0]);
        assert_ne!(east, south);
        for uv in east.iter().chain(south.iter()) {
            assert!((0.0..=1.0).contains(&uv[0]) && (0.0..=1.0).contains(&uv[1]));
        }
        let centre = east
            .iter()
            .fold([0.0; 2], |a, p| [a[0] + p[0] / 4.0, a[1] + p[1] / 4.0]);
        assert!((centre[0] - 0.5).abs() < 1e-6 && (centre[1] - 0.5).abs() < 1e-6);
    }
}
