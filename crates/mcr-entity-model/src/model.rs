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
