use std::collections::HashMap;

use glam::Vec3;

use crate::cube::{Vertex, unwrap_cube};
use crate::data::{LayerDef, PartDef};
use crate::pose::Pose;

pub type BoneId = u16;

#[derive(Debug, Clone)]
pub struct Bone {
    pub name: String,
    pub parent: Option<BoneId>,
    pub rest: Pose,
}

/// A baked layer: bones ordered parent-before-child, one shared vertex/index list skinned by bone.
#[derive(Debug, Clone)]
pub struct Model {
    pub bones: Vec<Bone>,
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub tex_size: [u32; 2],
    names: HashMap<String, BoneId>,
}

impl Model {
    pub fn from_layer(layer: &LayerDef) -> Self {
        let mut model = Model {
            bones: Vec::new(),
            vertices: Vec::new(),
            indices: Vec::new(),
            tex_size: [layer.tex_w, layer.tex_h],
            names: HashMap::new(),
        };
        model.add_part("root", &layer.root, None);
        model
    }

    fn add_part(&mut self, name: &str, part: &PartDef, parent: Option<BoneId>) {
        let id = self.bones.len() as BoneId;
        let p = part.pose;
        self.bones.push(Bone {
            name: name.to_owned(),
            parent,
            rest: Pose {
                pos: Vec3::new(p[0], p[1], p[2]),
                rot: Vec3::new(p[3], p[4], p[5]),
                scale: Vec3::new(p[6], p[7], p[8]),
            },
        });
        self.names.entry(name.to_owned()).or_insert(id);
        for cube in &part.cubes {
            unwrap_cube(
                cube,
                self.tex_size,
                id as u32,
                &mut self.vertices,
                &mut self.indices,
            );
        }
        for (child_name, child) in &part.children {
            self.add_part(child_name, child, Some(id));
        }
    }

    /// A single-bone model from ready-made geometry (item and block meshes); vertices use bone 0.
    pub fn from_geometry(vertices: Vec<Vertex>, indices: Vec<u32>) -> Self {
        Model {
            bones: vec![Bone {
                name: "root".to_owned(),
                parent: None,
                rest: crate::pose::Pose::IDENTITY,
            }],
            vertices,
            indices,
            tex_size: [1, 1],
            names: HashMap::from([("root".to_owned(), 0)]),
        }
    }

    /// A box centred on x/z with its feet at the entity origin once the standard entity transform
    /// is applied; scaled by the plan to hitbox size.
    pub fn unit_cube() -> Self {
        let (lo, hi) = (8.016f32, 24.016f32);
        let corners = |f: [[f32; 3]; 4], n: [f32; 3]| -> Vec<Vertex> {
            f.iter()
                .zip([[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]])
                .map(|(p, uv)| Vertex {
                    pos: *p,
                    bone: 0,
                    normal: n,
                    uv,
                })
                .collect()
        };
        let faces = [
            (
                [
                    [-8.0, lo, -8.0],
                    [8.0, lo, -8.0],
                    [8.0, hi, -8.0],
                    [-8.0, hi, -8.0],
                ],
                [0.0, 0.0, -1.0],
            ),
            (
                [
                    [8.0, lo, 8.0],
                    [-8.0, lo, 8.0],
                    [-8.0, hi, 8.0],
                    [8.0, hi, 8.0],
                ],
                [0.0, 0.0, 1.0],
            ),
            (
                [
                    [-8.0, lo, 8.0],
                    [-8.0, lo, -8.0],
                    [-8.0, hi, -8.0],
                    [-8.0, hi, 8.0],
                ],
                [-1.0, 0.0, 0.0],
            ),
            (
                [
                    [8.0, lo, -8.0],
                    [8.0, lo, 8.0],
                    [8.0, hi, 8.0],
                    [8.0, hi, -8.0],
                ],
                [1.0, 0.0, 0.0],
            ),
            (
                [
                    [-8.0, lo, 8.0],
                    [8.0, lo, 8.0],
                    [8.0, lo, -8.0],
                    [-8.0, lo, -8.0],
                ],
                [0.0, -1.0, 0.0],
            ),
            (
                [
                    [-8.0, hi, -8.0],
                    [8.0, hi, -8.0],
                    [8.0, hi, 8.0],
                    [-8.0, hi, 8.0],
                ],
                [0.0, 1.0, 0.0],
            ),
        ];
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        for (f, n) in faces {
            let base = vertices.len() as u32;
            vertices.extend(corners(f, n));
            indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        }
        Self::from_geometry(vertices, indices)
    }

    pub fn bone(&self, name: &str) -> Option<BoneId> {
        self.names.get(name).copied()
    }

    pub fn rest_pose(&self) -> Vec<Pose> {
        self.bones.iter().map(|b| b.rest).collect()
    }

    pub fn cube_count(&self) -> usize {
        self.vertices.len() / 24
    }
}
