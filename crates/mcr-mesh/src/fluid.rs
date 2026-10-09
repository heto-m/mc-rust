//! Water and lava surfaces, following vanilla's fluid renderer. Flow-direction texture rotation is
//! not implemented: tops always use the still sprite.

use mcr_render::{Face, ModelQuadDesc, SectionMesh};

use crate::cache::{ATTR_FULL_COLLISION, ATTR_HALF_TRANSPARENT, FluidKind, StateModel};
use crate::mesher::{Ctx, Mesher};
use crate::tint::{TINT_NONE, TINT_WATER};

const UP: [i32; 3] = [0, 1, 0];
const DOWN: [i32; 3] = [0, -1, 0];

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
    } else if ctx.attr(c) & ATTR_FULL_COLLISION == 0 {
        0.0
    } else {
        -1.0
    }
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

fn corner_height(ctx: &Ctx, kind: FluidKind, own: f32, a: f32, b: f32, corner: [i32; 3]) -> f32 {
    if a >= 1.0 || b >= 1.0 {
        return 1.0;
    }
    let mut acc = [0.0; 2];
    if a > 0.0 || b > 0.0 {
        let h = height_at(ctx, kind, corner);
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

fn max_light(a: (u8, u8), b: (u8, u8)) -> u8 {
    a.0.max(b.0) << 4 | a.1.max(b.1)
}

/// Emits the fluid at `cell`; returns whether any quad uses a biome tint.
pub(crate) fn mesh_fluid(
    mesher: &Mesher,
    ctx: &Ctx,
    cell: [i32; 3],
    sm: &StateModel,
    out: &mut SectionMesh,
) -> bool {
    let kind = sm.fluid.kind;
    let sprites = ctx.cache.fluid_sprites;
    let (still, flow) = match kind {
        FluidKind::Water => (sprites.water_still, sprites.water_flow),
        _ => (sprites.lava_still, sprites.lava_flow),
    };
    let translucent = kind == FluidKind::Water;
    let tint = if translucent { TINT_WATER } else { TINT_NONE };
    let own_occlusion = ctx.cache.occlusion(ctx.state(cell));
    let neighbor = |d: [i32; 3]| {
        let c = add(cell, d);
        (c, ctx.state(c))
    };
    let (up_cell, up_id) = neighbor(UP);
    let (down_cell, down_id) = neighbor(DOWN);

    let side_dirs: [([i32; 3], usize); 4] = [
        ([0, 0, -1], 5),
        ([0, 0, 1], 4),
        ([-1, 0, 0], 1),
        ([1, 0, 0], 0),
    ];
    let same = |id: u16| ctx.cache.state(id).fluid.kind == kind;
    let occluded_by = |id: u16, face: usize| ctx.cache.occlusion(id) >> face & 1 != 0;

    let render_up = !same(up_id);
    let render_down = !same(down_id) && own_occlusion >> 3 & 1 == 0 && !occluded_by(down_id, 2);
    let render_side: [bool; 4] = std::array::from_fn(|i| {
        let (_, id) = neighbor(side_dirs[i].0);
        !same(id) && own_occlusion >> side_dirs[i].1 & 1 == 0
    });
    if !(render_up || render_down || render_side.iter().any(|&b| b)) {
        return false;
    }

    let own = height_at(ctx, kind, cell);
    let h = |d: [i32; 3]| height_at(ctx, kind, add(cell, d));
    let (mut nw, mut sw, mut se, mut ne) = (1.0f32, 1.0f32, 1.0f32, 1.0f32);
    if own < 1.0 {
        let (n, s, e, w) = (h([0, 0, -1]), h([0, 0, 1]), h([1, 0, 0]), h([-1, 0, 0]));
        let corner = |dx, dz| add(cell, [dx, 0, dz]);
        ne = corner_height(ctx, kind, own, n, e, corner(1, -1));
        nw = corner_height(ctx, kind, own, n, w, corner(-1, -1));
        se = corner_height(ctx, kind, own, s, e, corner(1, 1));
        sw = corner_height(ctx, kind, own, s, w, corner(-1, 1));
    }

    let [cx, cy, cz] = cell.map(|v| v as f32);
    let top_light = max_light(ctx.light(cell), ctx.light(up_cell));
    let cell_xz = [cell[0] as u8, cell[2] as u8];
    let mut emit =
        |corners: [[f32; 3]; 4], uvs: [[f32; 2]; 4], layer: u16, light: u8, shade: Face| {
            // Vertex order is a cycle around the quad; slots are (0,0), (1,0), (0,1), (1,1).
            let order = [0usize, 1, 3, 2];
            let mut c = [[0.0; 3]; 4];
            let mut t = [[0.0; 2]; 4];
            for i in 0..4 {
                c[order[i]] = corners[i];
                t[order[i]] = uvs[i];
            }
            mesher.push_fluid_quad(
                &ModelQuadDesc {
                    corners: c,
                    uvs: t,
                    layer,
                    light,
                    shade_face: shade,
                    flags: 0,
                    ao: [3; 4],
                    tint,
                    cell: cell_xz,
                },
                translucent,
                out,
            );
        };

    if render_up {
        let under = |v: f32| v - 0.001;
        let min = nw.min(sw).min(se).min(ne);
        let ceiling_blocks = ctx.cache.occlusion(up_id) >> 3 & 1 != 0 && min >= 1.0;
        if !ceiling_blocks {
            emit(
                [
                    [cx, cy + under(nw), cz],
                    [cx, cy + under(sw), cz + 1.0],
                    [cx + 1.0, cy + under(se), cz + 1.0],
                    [cx + 1.0, cy + under(ne), cz],
                ],
                [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]],
                still,
                top_light,
                Face::PosY,
            );
        }
    }
    let bottom = if render_down { 0.001 } else { 0.0 };
    if render_down {
        let light = max_light(ctx.light(down_cell), (0, 0));
        emit(
            [
                [cx, cy + bottom, cz],
                [cx + 1.0, cy + bottom, cz],
                [cx + 1.0, cy + bottom, cz + 1.0],
                [cx, cy + bottom, cz + 1.0],
            ],
            [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            still,
            light,
            Face::NegY,
        );
    }

    for (i, (dir, face)) in side_dirs.iter().enumerate() {
        if !render_side[i] {
            continue;
        }
        let (h0, h1, x0, z0, x1, z1) = match i {
            0 => (nw, ne, cx, cz + 0.001, cx + 1.0, cz + 0.001),
            1 => (se, sw, cx + 1.0, cz + 1.0 - 0.001, cx, cz + 1.0 - 0.001),
            2 => (sw, nw, cx + 0.001, cz + 1.0, cx + 0.001, cz),
            _ => (ne, se, cx + 1.0 - 0.001, cz, cx + 1.0 - 0.001, cz + 1.0),
        };
        let (_, nid) = neighbor(*dir);
        let blocked_by_neighbor = occluded_by(nid, crate::faces::opposite(*face));
        if blocked_by_neighbor {
            continue;
        }
        let overlay = translucent && ctx.cache.attr(nid) & ATTR_HALF_TRANSPARENT != 0;
        let layer = if overlay { sprites.water_overlay } else { flow };
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
            top_light,
            Face::ALL[*face],
        );
    }
    translucent
}
