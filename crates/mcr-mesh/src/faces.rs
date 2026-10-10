//! Face indexing. Render faces follow `mcr_render::Face` (+X, -X, +Y, -Y, +Z, -Z); vanilla ordinals
//! follow `Direction` (down, up, north, south, west, east), which `mcr-data` and `mcr-assets` use.

pub const NORMAL: [[i32; 3]; 6] = [
    [1, 0, 0],
    [-1, 0, 0],
    [0, 1, 0],
    [0, -1, 0],
    [0, 0, 1],
    [0, 0, -1],
];

pub const RENDER_TO_VANILLA: [usize; 6] = [5, 4, 1, 0, 3, 2];
pub const VANILLA_TO_RENDER: [usize; 6] = [3, 2, 5, 4, 1, 0];

pub const VANILLA_OFFSET: [[i32; 3]; 6] = [
    [0, -1, 0],
    [0, 1, 0],
    [0, 0, -1],
    [0, 0, 1],
    [-1, 0, 0],
    [1, 0, 0],
];

/// Axes of a face's quad (u, v), matching the shader: X faces run (z, y), Y faces (x, z), Z faces (x, y).
pub const TANGENTS: [[usize; 2]; 3] = [[2, 1], [0, 2], [0, 1]];

pub const fn axis(face: usize) -> usize {
    face >> 1
}

pub const fn is_positive(face: usize) -> bool {
    face & 1 == 0
}

/// Texture coordinate of the corner `(cu, cv)` of a face quad, mirrored like vanilla's default UVs.
pub fn corner_uv(face: usize, cu: f32, cv: f32) -> [f32; 2] {
    let mirror_u = face == 0 || face == 5;
    let mirror_v = axis(face) != 1 || face == 3;
    [
        if mirror_u { 1.0 - cu } else { cu },
        if mirror_v { 1.0 - cv } else { cv },
    ]
}

/// Position of the corner `(cu, cv)` of the unit face of the block at `cell`.
pub fn corner_position(face: usize, cell: [i32; 3], cu: usize, cv: usize) -> [f32; 3] {
    let ax = axis(face);
    let [ua, va] = TANGENTS[ax];
    let mut p = [cell[0] as f32, cell[1] as f32, cell[2] as f32];
    if is_positive(face) {
        p[ax] += 1.0;
    }
    p[ua] += cu as f32;
    p[va] += cv as f32;
    p
}

pub fn render_face_from_vanilla(face: mcr_assets::Face) -> usize {
    VANILLA_TO_RENDER[face as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mappings_are_inverse() {
        for r in 0..6 {
            assert_eq!(VANILLA_TO_RENDER[RENDER_TO_VANILLA[r]], r);
            let v = RENDER_TO_VANILLA[r];
            assert_eq!(VANILLA_OFFSET[v], NORMAL[r]);
        }
    }
}
