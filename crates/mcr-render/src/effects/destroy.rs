use super::sky::FxVertex;

pub const MAX_DECALS: usize = 256;
pub const MAX_BOXES_PER_DECAL: usize = 16;
const GROW: f32 = 0.002;

/// A block being broken. `boxes` are `[min_x, min_y, min_z, max_x, max_y, max_z]` in block-local
/// coordinates (the block's model or outline shape); an empty list means the full cube.
#[derive(Debug, Clone, PartialEq)]
pub struct DestroyDecal {
    pub pos: [i32; 3],
    /// 0 to 9; larger values are ignored.
    pub stage: u8,
    pub boxes: Vec<[f32; 6]>,
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

pub(crate) fn build(decals: &[DestroyDecal], camera: [f64; 3]) -> Vec<FxVertex> {
    const FULL: [f32; 6] = [0.0, 0.0, 0.0, 1.0, 1.0, 1.0];
    let mut out = Vec::new();
    for decal in decals.iter().take(MAX_DECALS).filter(|d| d.stage < 10) {
        let origin = [
            (f64::from(decal.pos[0]) - camera[0]) as f32,
            (f64::from(decal.pos[1]) - camera[1]) as f32,
            (f64::from(decal.pos[2]) - camera[2]) as f32,
        ];
        let full = [FULL];
        let boxes: &[[f32; 6]] = if decal.boxes.is_empty() {
            &full
        } else {
            &decal.boxes[..decal.boxes.len().min(MAX_BOXES_PER_DECAL)]
        };
        let layer = f32::from(decal.stage) / 255.0;
        for b in boxes
            .iter()
            .filter(|b| b.iter().all(|v| v.is_finite() && v.abs() < 4.0))
        {
            for (face, &(a, v_axis, n_axis, positive)) in FACES.iter().enumerate() {
                let _ = face;
                let n_val = if positive {
                    b[3 + n_axis] + GROW
                } else {
                    b[n_axis] - GROW
                };
                for (ua, va) in [(0, 0), (1, 0), (1, 1), (0, 1)] {
                    let mut p = [0.0f32; 3];
                    let mut local = [0.0f32; 3];
                    p[n_axis] = n_val;
                    local[n_axis] = n_val;
                    local[a] = if ua == 1 { b[3 + a] } else { b[a] };
                    local[v_axis] = if va == 1 { b[3 + v_axis] } else { b[v_axis] };
                    let uv_u = if a == 1 { 1.0 - local[a] } else { local[a] };
                    let uv_v = if v_axis == 1 {
                        1.0 - local[v_axis]
                    } else {
                        local[v_axis]
                    };
                    for (k, c) in p.iter_mut().enumerate() {
                        *c = if k == n_axis { n_val } else { local[k] } + origin[k];
                    }
                    out.push(FxVertex::new(p, [uv_u, uv_v], [1.0, 1.0, 1.0, layer]));
                }
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
                boxes: Vec::new(),
            }],
            [0.0; 3],
        );
        assert_eq!(v.len(), 24);
    }

    #[test]
    fn out_of_range_stage_is_dropped() {
        let v = build(
            &[DestroyDecal {
                pos: [0, 0, 0],
                stage: 10,
                boxes: Vec::new(),
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
            })
            .collect();
        assert!(build(&many, [f64::MAX, 0.0, 0.0]).is_empty());
        let boxes = vec![[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]; 1000];
        let many: Vec<_> = (0..10_000)
            .map(|_| DestroyDecal {
                pos: [0; 3],
                stage: 5,
                boxes: boxes.clone(),
            })
            .collect();
        assert!(build(&many, [0.0; 3]).len() <= MAX_DECALS * MAX_BOXES_PER_DECAL * 24);
    }
}
