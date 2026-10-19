use crate::graph::SectionVisibility;
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
    /// One or more [`TintTable`]s back to back; the top byte of word `y` of the first names the table
    /// that blocks at section-local height `y` read.
    pub tints: Option<Box<[u32]>>,
    /// Face-to-face connectivity through non-opaque cells, kept even when the mesh is empty.
    pub visibility: SectionVisibility,
    /// The world camera position `translucent` is sorted back to front for.
    pub sort_camera: Option<[f64; 3]>,
}

/// The tint colour of `kind` (1 to 4) at a section-local block in `tables`.
pub fn tint_at(tables: &[u32], kind: u8, x: u8, y: u8, z: u8) -> u32 {
    let slot = tables
        .get(usize::from(y & 15))
        .map_or(0, |w| (w >> 24) as usize);
    let at = |slot: usize| {
        slot * TINT_TABLE_WORDS
            + (usize::from(kind).clamp(1, TINT_KINDS) - 1) * 256
            + usize::from(z & 15) * 16
            + usize::from(x & 15)
    };
    tables
        .get(at(slot))
        .or_else(|| tables.get(at(0)))
        .map_or(0, |c| c & 0xff_ffff)
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
        self.visibility = SectionVisibility::ALL;
        self.sort_camera = None;
    }
}
