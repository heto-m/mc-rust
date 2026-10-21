use crate::graph::SectionVisibility;
use crate::quad::{ModelQuad, PackedQuad};

pub const TINT_KINDS: usize = 4;
pub const TINT_TABLE_WORDS: usize = TINT_KINDS * 256;

/// Per-column biome tint colours (`r | g << 8 | b << 16`) for tint kinds 1 to 4, indexed
/// `kind - 1`, then `z * 16 + x`.
pub type TintTable = [u32; TINT_TABLE_WORDS];

/// One 16^3 section's geometry. The main pass draws `faces` and `model` (opaque and cutout); the
/// translucent pass draws `translucent`, which is always model quads. Opaque quads that lead a face
/// list or `model` draw first, without alpha testing ([`SectionMesh::partition_opaque`]).
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

    /// Moves opaque quads ahead of cutout ones in every face list and in `model`, keeping each group's
    /// order; the scratch vectors only carry capacity between calls.
    pub fn partition_opaque(&mut self, cube: &mut Vec<PackedQuad>, model: &mut Vec<ModelQuad>) {
        for face in &mut self.faces {
            stable_partition(face, cube, |q| q.is_opaque());
        }
        stable_partition(&mut self.model, model, ModelQuad::is_opaque);
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

fn stable_partition<T: Copy>(items: &mut Vec<T>, scratch: &mut Vec<T>, first: impl Fn(&T) -> bool) {
    let lead = items.iter().take_while(|q| first(q)).count();
    if items[lead..].iter().all(|q| !first(q)) {
        return;
    }
    scratch.clear();
    let mut kept = lead;
    for i in lead..items.len() {
        let q = items[i];
        if first(&q) {
            items[kept] = q;
            kept += 1;
        } else {
            scratch.push(q);
        }
    }
    items.truncate(kept);
    items.extend_from_slice(scratch);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_partition_keeps_group_order() {
        let mut v = vec![1, 2, 4, 3, 6, 5, 8];
        let mut scratch = Vec::new();
        stable_partition(&mut v, &mut scratch, |x| x % 2 == 0);
        assert_eq!(v, [2, 4, 6, 8, 1, 3, 5]);
        let mut sorted = vec![2, 4, 1];
        stable_partition(&mut sorted, &mut scratch, |x| x % 2 == 0);
        assert_eq!(sorted, [2, 4, 1]);
        let mut empty: Vec<i32> = Vec::new();
        stable_partition(&mut empty, &mut scratch, |x| x % 2 == 0);
        assert!(empty.is_empty());
    }
}
