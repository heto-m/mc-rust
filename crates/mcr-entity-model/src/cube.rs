use bytemuck::{Pod, Zeroable};

use crate::data::CubeDef;

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable, PartialEq)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub bone: u32,
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

#[derive(Clone, Copy)]
enum Face {
    Down,
    Up,
    West,
    North,
    East,
    South,
}

impl Face {
    const ALL: [Face; 6] = [
        Face::Down,
        Face::Up,
        Face::West,
        Face::North,
        Face::East,
        Face::South,
    ];

    fn name(self) -> &'static str {
        match self {
            Face::Down => "down",
            Face::Up => "up",
            Face::West => "west",
            Face::North => "north",
            Face::East => "east",
            Face::South => "south",
        }
    }

    fn normal(self, mirror: bool) -> [f32; 3] {
        let x = if mirror { -1.0 } else { 1.0 };
        match self {
            Face::Down => [0.0, -1.0, 0.0],
            Face::Up => [0.0, 1.0, 0.0],
            Face::West => [-x, 0.0, 0.0],
            Face::North => [0.0, 0.0, -1.0],
            Face::East => [x, 0.0, 0.0],
            Face::South => [0.0, 0.0, 1.0],
        }
    }
}

/// Appends the cube's quads (two triangles each) in model space, in sixteenths of a block.
pub fn unwrap_cube(
    cube: &CubeDef,
    tex_size: [u32; 2],
    bone: u32,
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u32>,
) {
    let [ox, oy, oz] = cube.origin;
    let [w, h, d] = cube.size;
    let [gx, gy, gz] = cube.grow;
    let (mut min_x, mut max_x) = (ox - gx, ox + w + gx);
    let (min_y, max_y) = (oy - gy, oy + h + gy);
    let (min_z, max_z) = (oz - gz, oz + d + gz);
    if cube.mirror {
        std::mem::swap(&mut min_x, &mut max_x);
    }
    let ts = cube.tex_scale.unwrap_or([1.0, 1.0]);
    let tw = tex_size[0] as f32 * ts[0];
    let th = tex_size[1] as f32 * ts[1];

    let t0 = [min_x, min_y, min_z];
    let t1 = [max_x, min_y, min_z];
    let t2 = [max_x, max_y, min_z];
    let t3 = [min_x, max_y, min_z];
    let l0 = [min_x, min_y, max_z];
    let l1 = [max_x, min_y, max_z];
    let l2 = [max_x, max_y, max_z];
    let l3 = [min_x, max_y, max_z];

    let (xo, yo) = (cube.uv[0].trunc(), cube.uv[1].trunc());
    let u0 = xo;
    let u1 = xo + d;
    let u2 = xo + d + w;
    let u22 = xo + d + w + w;
    let u3 = xo + d + w + d;
    let u4 = xo + d + w + d + w;
    let v0 = yo;
    let v1 = yo + d;
    let v2 = yo + d + h;

    for face in Face::ALL {
        if let Some(visible) = &cube.faces
            && !visible.iter().any(|f| f == face.name())
        {
            continue;
        }
        let (corners, [fu0, fv0, fu1, fv1]) = match face {
            Face::Down => ([l1, l0, t0, t1], [u1, v0, u2, v1]),
            Face::Up => ([t2, t3, l3, l2], [u2, v1, u22, v0]),
            Face::West => ([t0, l0, l3, t3], [u0, v1, u1, v2]),
            Face::North => ([t1, t0, t3, t2], [u1, v1, u2, v2]),
            Face::East => ([l1, t1, t2, l2], [u2, v1, u3, v2]),
            Face::South => ([l0, l1, l2, l3], [u3, v1, u4, v2]),
        };
        let uvs = [
            [fu1 / tw, fv0 / th],
            [fu0 / tw, fv0 / th],
            [fu0 / tw, fv1 / th],
            [fu1 / tw, fv1 / th],
        ];
        let mut quad: [usize; 4] = [0, 1, 2, 3];
        if cube.mirror {
            quad.reverse();
        }
        let normal = face.normal(cube.mirror);
        let base = vertices.len() as u32;
        for i in quad {
            vertices.push(Vertex {
                pos: corners[i],
                bone,
                normal,
                uv: uvs[i],
            });
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn head() -> CubeDef {
        CubeDef {
            origin: [-4.0, -8.0, -4.0],
            size: [8.0, 8.0, 8.0],
            uv: [0.0, 0.0],
            grow: [0.0; 3],
            mirror: false,
            tex_scale: None,
            faces: None,
        }
    }

    fn face_uv_rect(vertices: &[Vertex], face: usize, tex: [f32; 2]) -> [f32; 4] {
        let q = &vertices[face * 4..face * 4 + 4];
        let us: Vec<f32> = q.iter().map(|v| v.uv[0] * tex[0]).collect();
        let vs: Vec<f32> = q.iter().map(|v| v.uv[1] * tex[1]).collect();
        let min = |a: &[f32]| a.iter().copied().fold(f32::MAX, f32::min);
        let max = |a: &[f32]| a.iter().copied().fold(f32::MIN, f32::max);
        [min(&us), min(&vs), max(&us), max(&vs)]
    }

    #[test]
    fn head_cube_faces_match_vanilla_layout() {
        let (mut v, mut i) = (vec![], vec![]);
        unwrap_cube(&head(), [64, 32], 0, &mut v, &mut i);
        assert_eq!((v.len(), i.len()), (24, 36));
        let tex = [64.0, 32.0];
        assert_eq!(face_uv_rect(&v, 0, tex), [8.0, 0.0, 16.0, 8.0]);
        assert_eq!(face_uv_rect(&v, 1, tex), [16.0, 0.0, 24.0, 8.0]);
        assert_eq!(face_uv_rect(&v, 2, tex), [0.0, 8.0, 8.0, 16.0]);
        assert_eq!(face_uv_rect(&v, 3, tex), [8.0, 8.0, 16.0, 16.0]);
        assert_eq!(face_uv_rect(&v, 4, tex), [16.0, 8.0, 24.0, 16.0]);
        assert_eq!(face_uv_rect(&v, 5, tex), [24.0, 8.0, 32.0, 16.0]);
    }

    #[test]
    fn mirror_swaps_x_and_flips_x_normals() {
        let mut cube = head();
        cube.mirror = true;
        let (mut v, mut i) = (vec![], vec![]);
        unwrap_cube(&cube, [64, 32], 0, &mut v, &mut i);
        assert_eq!(v[8].normal, [1.0, 0.0, 0.0]);
        assert_eq!(v[16].normal, [-1.0, 0.0, 0.0]);
        assert_eq!(v[12].normal, [0.0, 0.0, -1.0]);
    }

    #[test]
    fn grow_inflates_and_hidden_faces_are_skipped() {
        let mut cube = head();
        cube.grow = [0.5; 3];
        cube.faces = Some(vec!["up".into(), "down".into()]);
        let (mut v, mut i) = (vec![], vec![]);
        unwrap_cube(&cube, [64, 32], 2, &mut v, &mut i);
        assert_eq!(v.len(), 8);
        assert!(v.iter().all(|p| p.bone == 2));
        let max_x = v.iter().map(|p| p.pos[0]).fold(f32::MIN, f32::max);
        assert_eq!(max_x, 4.5);
    }
}
