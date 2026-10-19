use super::sky::FxVertex;

pub const MAX_DECALS: usize = 256;
pub const MAX_BOXES_PER_DECAL: usize = 16;
/// Quads one decal draws at most, the same budget as its boxes.
pub const MAX_QUADS_PER_DECAL: usize = MAX_BOXES_PER_DECAL * 6;
const GROW: f32 = 0.002;

/// A block being broken. `quads` are the block model's quads in block-local coordinates, like vanilla's
/// crumbling overlay; without them, `boxes` (`[min_x, min_y, min_z, max_x, max_y, max_z]`, an empty
/// list meaning the full cube) stand in.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DestroyDecal {
    pub pos: [i32; 3],
    /// 0 to 9; larger values are ignored.
    pub stage: u8,
    pub boxes: Vec<[f32; 6]>,
    pub quads: Vec<[[f32; 3]; 4]>,
}

/// Per-face tangent axes with `u x v` equal to the outward normal, so (0,0),(1,0),(1,1),(0,1) wind counter-clockwise from outside.
const FACES: [(usize, usize, usize, bool); 6] = [
    (0, 2, 1, false),
    (1, 2, 0, true),
    (2, 1, 0, false),
    (0, 1, 2, true),
    (1, 0, 2, false),
    (2, 0, 1, true),
];

/// The crack texture coordinate of a block-local point on a face along `(n_axis, positive)`.
fn decal_uv(local: [f32; 3], n_axis: usize, positive: bool) -> [f32; 2] {
    let &(a, v_axis, _, _) = FACES
        .iter()
        .find(|f| f.2 == n_axis && f.3 == positive)
        .expect("every axis and sign has a face");
    let flip = |axis: usize, v: f32| if axis == 1 { 1.0 - v } else { v };
    [flip(a, local[a]), flip(v_axis, local[v_axis])]
}

fn box_quads(b: &[f32; 6], out: &mut Vec<[[f32; 3]; 4]>) {
    for &(a, v_axis, n_axis, positive) in &FACES {
        let n_val = if positive { b[3 + n_axis] } else { b[n_axis] };
        out.push([(0, 0), (1, 0), (1, 1), (0, 1)].map(|(ua, va)| {
            let mut p = [0.0f32; 3];
            p[n_axis] = n_val;
            p[a] = if ua == 1 { b[3 + a] } else { b[a] };
            p[v_axis] = if va == 1 { b[3 + v_axis] } else { b[v_axis] };
            p
        }));
    }
}

pub(crate) fn build(decals: &[DestroyDecal], camera: [f64; 3]) -> Vec<FxVertex> {
    const FULL: [f32; 6] = [0.0, 0.0, 0.0, 1.0, 1.0, 1.0];
    let mut out = Vec::new();
    let mut quads = Vec::new();
    for decal in decals.iter().take(MAX_DECALS).filter(|d| d.stage < 10) {
        let origin = [
            (f64::from(decal.pos[0]) - camera[0]) as f32,
            (f64::from(decal.pos[1]) - camera[1]) as f32,
            (f64::from(decal.pos[2]) - camera[2]) as f32,
        ];
        quads.clear();
        if decal.quads.is_empty() {
            let full = [FULL];
            let boxes: &[[f32; 6]] = if decal.boxes.is_empty() {
                &full
            } else {
                &decal.boxes[..decal.boxes.len().min(MAX_BOXES_PER_DECAL)]
            };
            for b in boxes {
                box_quads(b, &mut quads);
            }
        } else {
            quads.extend(decal.quads.iter().take(MAX_QUADS_PER_DECAL));
        }
        let layer = f32::from(decal.stage) / 255.0;
        for q in quads
            .iter()
            .filter(|q| q.iter().flatten().all(|v| v.is_finite() && v.abs() < 4.0))
        {
            let e1 = [q[1][0] - q[0][0], q[1][1] - q[0][1], q[1][2] - q[0][2]];
            let e2 = [q[2][0] - q[0][0], q[2][1] - q[0][1], q[2][2] - q[0][2]];
            let normal = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            let len = normal.iter().map(|v| v * v).sum::<f32>().sqrt();
            if len < 1e-9 {
                continue;
            }
            let n_axis = (0..3)
                .max_by(|&i, &j| normal[i].abs().total_cmp(&normal[j].abs()))
                .unwrap_or(1);
            let positive = normal[n_axis] > 0.0;
            for corner in q {
                let p: [f32; 3] =
                    std::array::from_fn(|k| corner[k] + normal[k] / len * GROW + origin[k]);
                let uv = decal_uv(*corner, n_axis, positive);
                out.push(FxVertex::new(p, uv, [1.0, 1.0, 1.0, layer]));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_cube_has_six_quads() {
        let v = build(
            &[DestroyDecal {
                pos: [0, 0, 0],
                stage: 3,
                ..DestroyDecal::default()
            }],
            [0.0; 3],
        );
        assert_eq!(v.len(), 24);
    }

    #[test]
    fn model_quads_replace_the_boxes() {
        let slab_top = [
            [0.0, 0.5, 0.0],
            [0.0, 0.5, 1.0],
            [1.0, 0.5, 1.0],
            [1.0, 0.5, 0.0],
        ];
        let v = build(
            &[DestroyDecal {
                pos: [0, 0, 0],
                stage: 3,
                boxes: vec![[0.0; 6]],
                quads: vec![slab_top],
            }],
            [0.0; 3],
        );
        assert_eq!(v.len(), 4);
        assert!(v.iter().all(|p| (p.pos[1] - 0.5 - GROW).abs() < 1e-6));
    }

    #[test]
    fn out_of_range_stage_is_dropped() {
        let v = build(
            &[DestroyDecal {
                pos: [0, 0, 0],
                stage: 10,
                ..DestroyDecal::default()
            }],
            [0.0; 3],
        );
        assert!(v.is_empty());
    }
}

#[cfg(test)]
mod hostile_tests {
    use super::*;

    #[test]
    fn caps_and_filters_hostile_decals() {
        let many: Vec<_> = (0..10_000)
            .map(|i| DestroyDecal {
                pos: [i32::MAX, i32::MIN, i],
                stage: (i % 256) as u8,
                boxes: vec![[f32::NAN; 6]; 1000],
                quads: Vec::new(),
            })
            .collect();
        assert!(build(&many, [f64::MAX, 0.0, 0.0]).is_empty());
        let boxes = vec![[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]; 1000];
        let many: Vec<_> = (0..10_000)
            .map(|_| DestroyDecal {
                pos: [0; 3],
                stage: 5,
                boxes: boxes.clone(),
                quads: vec![[[0.5; 3]; 4]; 1000],
            })
            .collect();
        assert!(build(&many, [0.0; 3]).len() <= MAX_DECALS * MAX_BOXES_PER_DECAL * 24);
    }
}
