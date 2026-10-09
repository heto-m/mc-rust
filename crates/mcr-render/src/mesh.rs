use crate::quad::{ModelQuad, PackedQuad};

pub const TINT_KINDS: usize = 4;
pub const TINT_TABLE_WORDS: usize = TINT_KINDS * 256;

/// Per-column biome tint colours (`r | g << 8 | b << 16`) for tint kinds 1 to 4, indexed
/// `kind - 1`, then `z * 16 + x`.
pub type TintTable = [u32; TINT_TABLE_WORDS];

/// One 16^3 section's geometry. The main pass draws `faces` and `model` (opaque and cutout); the
/// translucent pass draws `translucent`, which is always model quads.
#[derive(Debug, Default, Clone)]
pub struct SectionMesh {
    pub faces: [Vec<PackedQuad>; 6],
    pub model: Vec<ModelQuad>,
    pub translucent: Vec<ModelQuad>,
    pub tints: Option<Box<TintTable>>,
}

impl SectionMesh {
    pub fn push_cube(&mut self, quad: PackedQuad) {
        self.faces[quad.face() as usize].push(quad);
    }

    pub fn cube_count(&self) -> usize {
        self.faces.iter().map(Vec::len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.cube_count() == 0 && self.model.is_empty() && self.translucent.is_empty()
    }

    pub fn clear(&mut self) {
        self.faces.iter_mut().for_each(Vec::clear);
        self.model.clear();
        self.translucent.clear();
        self.tints = None;
    }
}
