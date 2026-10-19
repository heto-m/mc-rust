//! Synthetic heightmap terrain and a deliberately simple mesher, used by tests, benches and the app
//! until the real world and meshing crates land.

use std::sync::atomic::{AtomicUsize, Ordering};

use crate::mesh::SectionMesh;
use crate::quad::{
    CubeQuad, Face, LIGHT_FULL_SKY, ModelQuad, ModelQuadDesc, PackedQuad, pack_light,
};

pub const GRASS_TOP: u16 = 0;
pub const GRASS_SIDE: u16 = 1;
pub const DIRT: u16 = 2;
pub const STONE: u16 = 3;
pub const SAND: u16 = 4;
pub const SNOW: u16 = 5;
pub const SLAB: u16 = 6;
pub const SECTION_LAYERS: i32 = 8;

#[derive(Debug, Clone, Copy)]
pub struct Terrain {
    pub seed: u32,
}

fn hash2(seed: u32, x: i32, z: i32) -> u32 {
    let mut h = seed ^ (x as u32).wrapping_mul(0x9e37_79b1) ^ (z as u32).wrapping_mul(0x85eb_ca6b);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297a_2d39);
    h ^ h >> 15
}

fn value_noise(seed: u32, x: f32, z: f32) -> f32 {
    let (xi, zi) = (x.floor(), z.floor());
    let (fx, fz) = (x - xi, z - zi);
    let s = |t: f32| t * t * (3.0 - 2.0 * t);
    let at =
        |dx: i32, dz: i32| (hash2(seed, xi as i32 + dx, zi as i32 + dz) & 0xffff) as f32 / 65535.0;
    let top = at(0, 0) + (at(1, 0) - at(0, 0)) * s(fx);
    let bottom = at(0, 1) + (at(1, 1) - at(0, 1)) * s(fx);
    top + (bottom - top) * s(fz)
}

impl Terrain {
    pub fn height(&self, x: i32, z: i32) -> i32 {
        let (fx, fz) = (x as f32, z as f32);
        let n = 0.6 * value_noise(self.seed, fx / 64.0, fz / 64.0)
            + 0.3 * value_noise(self.seed ^ 1, fx / 24.0, fz / 24.0)
            + 0.1 * value_noise(self.seed ^ 2, fx / 8.0, fz / 8.0);
        (36.0 + n * 70.0) as i32
    }

    fn has_slab(&self, x: i32, z: i32) -> bool {
        hash2(self.seed ^ 9, x, z).is_multiple_of(61)
    }
}

struct Heights {
    h: [i32; 18 * 18],
}

impl Heights {
    fn new(t: &Terrain, ox: i32, oz: i32) -> Self {
        let mut h = [0; 18 * 18];
        for z in 0..18 {
            for x in 0..18 {
                h[z * 18 + x] = t.height(ox + x as i32 - 1, oz + z as i32 - 1);
            }
        }
        Self { h }
    }

    fn at(&self, x: i32, z: i32) -> i32 {
        self.h[((z + 1) * 18 + x + 1) as usize]
    }

    fn solid(&self, x: i32, y: i32, z: i32) -> bool {
        y <= self.at(x, z)
    }
}

fn top_layer(h: i32) -> u16 {
    match h {
        ..=40 => SAND,
        96.. => SNOW,
        _ => GRASS_TOP,
    }
}

fn side_layer(y: i32, h: i32) -> u16 {
    if y == h {
        if h <= 40 { SAND } else { GRASS_SIDE }
    } else if y > h - 4 {
        DIRT
    } else {
        STONE
    }
}

const TANGENTS: [(usize, usize); 3] = [(2, 1), (0, 2), (0, 1)];

fn ao_corners(heights: &Heights, pos: [i32; 3], face: Face) -> [u8; 4] {
    let n = face.normal();
    let (ua, va) = TANGENTS[face as usize / 2];
    let solid = |d: [i32; 3]| {
        heights.solid(
            pos[0] + n[0] + d[0],
            pos[1] + n[1] + d[1],
            pos[2] + n[2] + d[2],
        )
    };
    std::array::from_fn(|c| {
        let su = if c & 1 == 0 { -1 } else { 1 };
        let sv = if c & 2 == 0 { -1 } else { 1 };
        let mut du = [0; 3];
        du[ua] = su;
        let mut dv = [0; 3];
        dv[va] = sv;
        let mut duv = du;
        duv[va] = sv;
        let (s1, s2, corner) = (solid(du), solid(dv), solid(duv));
        if s1 && s2 {
            1
        } else {
            4 - (u8::from(s1) + u8::from(s2) + u8::from(corner))
        }
    })
}

fn model_box(min: [f32; 3], max: [f32; 3], layer: u16) -> Vec<ModelQuad> {
    let mut out = Vec::new();
    for face in [Face::PosX, Face::NegX, Face::PosY, Face::PosZ, Face::NegZ] {
        let axis = face as usize / 2;
        let (ua, va) = TANGENTS[axis];
        let plane = if (face as usize).is_multiple_of(2) {
            max[axis]
        } else {
            min[axis]
        };
        let corners = std::array::from_fn(|c| {
            let mut p = [0.0; 3];
            p[axis] = plane;
            p[ua] = if c & 1 == 0 { min[ua] } else { max[ua] };
            p[va] = if c & 2 == 0 { min[va] } else { max[va] };
            p
        });
        let (w, h) = (max[ua] - min[ua], max[va] - min[va]);
        let desc = ModelQuadDesc {
            corners,
            uvs: [[0.0, h], [w, h], [0.0, 0.0], [w, 0.0]],
            layer,
            light: [LIGHT_FULL_SKY; 4],
            shade_face: face,
            flags: 0,
            ao: [63; 4],
            tint: 0,
            cell: [min[0] as u8, min[1] as u8, min[2] as u8],
        };
        out.push(ModelQuad::encode(&desc).expect("box within section bounds"));
    }
    out
}

pub fn mesh_section(terrain: &Terrain, sx: i32, sy: i32, sz: i32) -> SectionMesh {
    let (ox, oy, oz) = (sx * 16, sy * 16, sz * 16);
    let heights = Heights::new(terrain, ox, oz);
    let mut mesh = SectionMesh::default();
    let max_h = *heights.h.iter().max().expect("non-empty");
    if oy > max_h + 1 {
        return mesh;
    }

    let push =
        |mesh: &mut SectionMesh, x: i32, y: i32, z: i32, face: Face, layer: u16, light: u16| {
            let q = CubeQuad {
                x: x as u8,
                y: (y - oy) as u8,
                z: z as u8,
                face,
                width: 1,
                height: 1,
                layer,
                ao: ao_corners(&heights, [x, y, z], face),
                light: [light; 4],
                tint: 0,
                rotation: 0,
                opaque: false,
            };
            mesh.push_cube(PackedQuad::encode(&q).expect("quad fields in range"));
        };

    for z in 0..16 {
        for x in 0..16 {
            let h = heights.at(x, z);
            let lowest_open = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                .iter()
                .map(|(dx, dz)| heights.at(x + dx, z + dz))
                .min()
                .expect("four neighbours");
            if (oy..oy + 16).contains(&h) {
                push(&mut mesh, x, h, z, Face::PosY, top_layer(h), LIGHT_FULL_SKY);
            }
            for y in (lowest_open + 1).max(oy)..=h.min(oy + 15) {
                let layer = side_layer(y, h);
                for (face, dx, dz) in [
                    (Face::PosX, 1, 0),
                    (Face::NegX, -1, 0),
                    (Face::PosZ, 0, 1),
                    (Face::NegZ, 0, -1),
                ] {
                    if y > heights.at(x + dx, z + dz) {
                        push(&mut mesh, x, y, z, face, layer, pack_light(192, 0));
                    }
                }
            }
            if terrain.has_slab(ox + x, oz + z) && (oy..oy + 16).contains(&(h + 1)) {
                let (lx, ly, lz) = (x as f32, (h + 1 - oy) as f32, z as f32);
                mesh.model.extend(model_box(
                    [lx, ly, lz],
                    [lx + 1.0, ly + 0.5, lz + 1.0],
                    SLAB,
                ));
            }
        }
    }
    mesh
}

/// Meshes every section within `radius` chunks of the origin column using `threads` workers.
pub fn generate_world(
    terrain: &Terrain,
    radius: i32,
    threads: usize,
) -> Vec<([i32; 3], SectionMesh)> {
    let side = 2 * radius + 1;
    let columns = (side * side) as usize;
    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads.max(1))
            .map(|_| {
                scope.spawn(|| {
                    let mut local = Vec::new();
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        if i >= columns {
                            break;
                        }
                        let (cx, cz) = (i as i32 % side - radius, i as i32 / side - radius);
                        for sy in 0..SECTION_LAYERS {
                            let mesh = mesh_section(terrain, cx, sy, cz);
                            if !mesh.is_empty() {
                                local.push(([cx, sy, cz], mesh));
                            }
                        }
                    }
                    local
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|w| w.join().expect("mesher thread"))
            .collect::<Vec<_>>()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terrain_is_deterministic_and_bounded() {
        let t = Terrain { seed: 1 };
        for (x, z) in [(0, 0), (-500, 300), (12345, -9876)] {
            assert_eq!(t.height(x, z), t.height(x, z));
            assert!((36..=106).contains(&t.height(x, z)));
        }
    }

    #[test]
    fn mesh_has_top_faces_for_every_column() {
        let t = Terrain { seed: 3 };
        let mut tops = 0;
        for sy in 0..SECTION_LAYERS {
            tops += mesh_section(&t, 2, sy, -3).faces[Face::PosY as usize].len();
        }
        assert_eq!(tops, 256);
    }

    #[test]
    fn no_bottom_faces_and_ao_in_range() {
        let t = Terrain { seed: 5 };
        let m = mesh_section(&t, 0, 3, 0);
        assert!(m.faces[Face::NegY as usize].is_empty());
        for q in m.faces.iter().flatten() {
            assert!(q.decode().ao.iter().all(|&a| a <= 3));
        }
    }

    #[test]
    fn generate_world_covers_all_columns() {
        let w = generate_world(&Terrain { seed: 2 }, 2, 2);
        let columns: std::collections::HashSet<_> = w.iter().map(|(k, _)| (k[0], k[2])).collect();
        assert_eq!(columns.len(), 25);
    }
}
