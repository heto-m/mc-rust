use crate::quad::{ModelQuad, PackedQuad};

/// One 16^3 section's geometry: cube quads grouped by face direction plus free-form model quads.
#[derive(Debug, Default, Clone)]
pub struct SectionMesh {
    pub faces: [Vec<PackedQuad>; 6],
    pub model: Vec<ModelQuad>,
}

impl SectionMesh {
    pub fn push_cube(&mut self, quad: PackedQuad) {
        self.faces[quad.face() as usize].push(quad);
    }

    pub fn cube_count(&self) -> usize {
        self.faces.iter().map(Vec::len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.cube_count() == 0 && self.model.is_empty()
    }
}
