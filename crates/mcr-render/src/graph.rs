//! Section visibility graph: which faces of a section see each other through non-opaque cells, and a
//! breadth-first search from the camera section that only enters a neighbour through a connected face
//! and only moves away from the camera.

use rustc_hash::FxHashMap;

/// Face indices follow [`crate::Face`]: +X, -X, +Y, -Y, +Z, -Z.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SectionVisibility(u64);

const ROW: u64 = 0x3f;

impl SectionVisibility {
    pub const ALL: Self = Self((1 << 36) - 1);
    pub const NONE: Self = Self(0);

    pub fn connect(&mut self, a: usize, b: usize) {
        self.0 |= 1 << (6 * a + b) | 1 << (6 * b + a);
    }

    /// Connects every pair of faces in `faces` (bit per face).
    pub fn connect_all(&mut self, faces: u8) {
        for a in 0..6 {
            if faces >> a & 1 != 0 {
                self.0 |= u64::from(faces) << (6 * a);
            }
        }
    }

    pub fn connected(self, a: usize, b: usize) -> bool {
        self.0 >> (6 * a + b) & 1 != 0
    }

    /// Faces reachable after entering through any face in `entered` (bit per face).
    pub fn exits(self, entered: u8) -> u8 {
        let mut faces = entered & ROW as u8;
        let mut out = 0;
        while faces != 0 {
            let f = faces.trailing_zeros();
            out |= (self.0 >> (6 * f) & ROW) as u8;
            faces &= faces - 1;
        }
        out
    }
}

impl Default for SectionVisibility {
    fn default() -> Self {
        Self::ALL
    }
}

/// Sections searched around the camera along each horizontal axis and vertically.
const REACH_XZ: i32 = 130;
const REACH_Y: i32 = 64;
const NO_SLOT: u32 = u32::MAX;

#[derive(Debug, Clone, Copy)]
struct Node {
    vis: SectionVisibility,
    slot: u32,
}

const OPEN: Node = Node {
    vis: SectionVisibility::ALL,
    slot: NO_SLOT,
};

type Bounds = ([i32; 3], [i32; 3]);

#[derive(Debug, Default)]
pub(crate) struct VisibilityGraph {
    nodes: FxHashMap<[i32; 3], Node>,
    dirty: bool,
    origin: Option<[i32; 3]>,
    /// Bounds of `nodes`; `None` after a removal until the next search recomputes it.
    bounds: Option<Bounds>,
    /// Dense copy of `nodes` over `grid_box`, rebuilt only when nodes change or the box moves.
    vis: Vec<SectionVisibility>,
    slots: Vec<u32>,
    grid_box: Option<Bounds>,
    exits: Vec<u8>,
    reachable: Vec<u32>,
    /// One bit per section of `grid_box` that the last search entered, at `x + z * dx + y * dx * dz`.
    reached: Vec<u64>,
    generation: u64,
}

/// The sections the last graph search reached, over a box in section coordinates.
#[derive(Debug, Clone, Copy)]
pub struct ReachedSections<'a> {
    pub origin: [i32; 3],
    pub dims: [u32; 3],
    /// Bit `x + z * dx + y * dx * dz` of the box, relative to `origin`.
    pub bits: &'a [u64],
    /// Changes whenever the search ran again.
    pub generation: u64,
}

impl ReachedSections<'_> {
    pub fn contains(&self, section: [i32; 3]) -> bool {
        let rel = [0, 1, 2].map(|a| i64::from(section[a]) - i64::from(self.origin[a]));
        if (0..3).any(|a| rel[a] < 0 || rel[a] >= i64::from(self.dims[a])) {
            return true;
        }
        let [dx, _, dz] = self.dims.map(u64::from);
        let i = rel[0] as u64 + rel[2] as u64 * dx + rel[1] as u64 * dx * dz;
        self.bits
            .get((i / 64) as usize)
            .is_none_or(|w| w >> (i % 64) & 1 != 0)
    }
}

impl VisibilityGraph {
    /// Mirrors a node change into the dense grid when it lies inside it; otherwise drops the grid.
    fn changed(&mut self, key: [i32; 3], node: Node) {
        self.dirty = true;
        let Some((lo, hi)) = self.grid_box else {
            return;
        };
        if (0..3).all(|a| (lo[a]..=hi[a]).contains(&key[a])) {
            let w = (hi[0] - lo[0] + 1) as usize;
            let d = (hi[2] - lo[2] + 1) as usize;
            let [x, y, z] = [0, 1, 2].map(|a| (key[a] - lo[a]) as usize);
            let i = x + z * w + y * w * d;
            self.vis[i] = node.vis;
            self.slots[i] = node.slot;
        } else {
            self.grid_box = None;
        }
    }

    fn added(&mut self, key: [i32; 3], node: Node) {
        if let Some((lo, hi)) = &mut self.bounds {
            for a in 0..3 {
                lo[a] = lo[a].min(key[a]);
                hi[a] = hi[a].max(key[a]);
            }
        }
        self.nodes.insert(key, node);
        self.changed(key, node);
    }

    pub(crate) fn set(&mut self, key: [i32; 3], vis: SectionVisibility) {
        match self.nodes.get_mut(&key) {
            Some(node) if node.vis == vis => {}
            Some(node) => {
                node.vis = vis;
                let node = *node;
                self.changed(key, node);
            }
            None => self.added(key, Node { vis, slot: NO_SLOT }),
        }
    }

    /// Records the GPU slot drawing `key`; `None` keeps the node for connectivity only.
    pub(crate) fn set_slot(&mut self, key: [i32; 3], slot: Option<u32>) {
        let slot = slot.unwrap_or(NO_SLOT);
        match self.nodes.get_mut(&key) {
            Some(node) if node.slot == slot => {}
            Some(node) => {
                node.slot = slot;
                let node = *node;
                self.changed(key, node);
            }
            None if slot != NO_SLOT => self.added(key, Node { slot, ..OPEN }),
            None => {}
        }
    }

    pub(crate) fn remove(&mut self, key: [i32; 3]) {
        if self.nodes.remove(&key).is_some() {
            self.bounds = None;
            self.changed(key, OPEN);
        }
    }

    pub(crate) fn invalidate(&mut self) {
        self.dirty = true;
    }

    /// From the last [`Self::update`].
    pub(crate) fn reachable(&self, slot: u32) -> bool {
        self.reachable
            .get((slot / 32) as usize)
            .is_some_and(|w| w >> (slot % 32) & 1 != 0)
    }

    /// From the last [`Self::update`]; `None` before the first search.
    pub(crate) fn reached(&self) -> Option<ReachedSections<'_>> {
        let (lo, hi) = self.grid_box?;
        Some(ReachedSections {
            origin: lo,
            dims: [0, 1, 2].map(|a| (hi[a] - lo[a] + 1) as u32),
            bits: &self.reached,
            generation: self.generation,
        })
    }

    pub(crate) fn needs_update(&self, camera: [i32; 3]) -> bool {
        self.dirty || self.origin != Some(camera)
    }

    fn node_bounds(&mut self) -> Option<Bounds> {
        if self.bounds.is_none() && !self.nodes.is_empty() {
            let mut lo = [i32::MAX; 3];
            let mut hi = [i32::MIN; 3];
            for k in self.nodes.keys() {
                for a in 0..3 {
                    lo[a] = lo[a].min(k[a]);
                    hi[a] = hi[a].max(k[a]);
                }
            }
            self.bounds = Some((lo, hi));
        }
        self.bounds
    }

    /// Bitset over slots of the sections reachable from `camera` (section coordinates). Sections the
    /// graph has never seen count as open, so unmeshed air never blocks the search.
    ///
    /// Every step moves away from the camera, so within each octant around the camera a section can
    /// only be entered from its three neighbours nearer the camera; sweeping each octant outward
    /// replaces a queue-driven search.
    pub(crate) fn update(&mut self, camera: [i32; 3], slot_count: u32) -> &[u32] {
        self.dirty = false;
        self.origin = Some(camera);
        let (mut lo, mut hi) = self.node_bounds().unwrap_or((camera, camera));
        let reach = [REACH_XZ, REACH_Y, REACH_XZ];
        for a in 0..3 {
            lo[a] = lo[a].min(camera[a]).max(camera[a] - reach[a]);
            hi[a] = hi[a].max(camera[a]).min(camera[a] + reach[a]);
        }
        let dims = [0, 1, 2].map(|a| (hi[a] - lo[a] + 1) as usize);
        let stride = [1, dims[0] * dims[2], dims[0]];
        let volume = dims[0] * dims[1] * dims[2];
        if self.grid_box != Some((lo, hi)) {
            self.grid_box = Some((lo, hi));
            self.vis.clear();
            self.vis.resize(volume, SectionVisibility::ALL);
            self.slots.clear();
            self.slots.resize(volume, NO_SLOT);
            for (k, node) in &self.nodes {
                if (0..3).all(|a| (lo[a]..=hi[a]).contains(&k[a])) {
                    let i: usize = (0..3).map(|a| (k[a] - lo[a]) as usize * stride[a]).sum();
                    self.vis[i] = node.vis;
                    self.slots[i] = node.slot;
                }
            }
        }
        self.exits.clear();
        self.exits.resize(volume, 0);
        self.reachable.clear();
        self.reachable.resize(slot_count.div_ceil(32) as usize, 0);
        self.reached.clear();
        self.reached.resize(volume.div_ceil(64), 0);
        self.generation += 1;

        let c = [0, 1, 2].map(|a| (camera[a] - lo[a]) as usize);
        let start = c[0] + c[1] * stride[1] + c[2] * stride[2];
        let extent = |a: usize, forward: bool| if forward { dims[a] - 1 - c[a] } else { c[a] };
        for octant in 0..8u8 {
            let fwd = [octant & 1 != 0, octant & 2 != 0, octant & 4 != 0];
            let [ex, ey, ez] = [0, 1, 2].map(|a| extent(a, fwd[a]));
            // Moving outward along axis a in this octant is face 2a (+) or 2a+1 (-).
            let out = [0, 1, 2].map(|a| 2 * a + usize::from(!fwd[a]));
            let step = [0, 1, 2].map(|a| {
                let s = stride[a] as isize;
                if fwd[a] { s } else { -s }
            });
            let bit = out.map(|f| 1u8 << f);
            let enter = out.map(|f| 1u8 << (f ^ 1));
            for dy in 0..=ey {
                for dz in 0..=ez {
                    let mut i = start as isize + dy as isize * step[1] + dz as isize * step[2];
                    for dx in 0..=ex {
                        let at = i as usize;
                        i += step[0];
                        let mut entered = 0u8;
                        if dx > 0 && self.exits[(at as isize - step[0]) as usize] & bit[0] != 0 {
                            entered |= enter[0];
                        }
                        if dy > 0 && self.exits[(at as isize - step[1]) as usize] & bit[1] != 0 {
                            entered |= enter[1];
                        }
                        if dz > 0 && self.exits[(at as isize - step[2]) as usize] & bit[2] != 0 {
                            entered |= enter[2];
                        }
                        let exits = if at == start {
                            ROW as u8
                        } else if entered == 0 {
                            continue;
                        } else {
                            self.vis[at].exits(entered)
                        };
                        self.exits[at] = exits;
                        // The dense index is x + z * dx + y * dx * dz, the shared layout.
                        self.reached[at / 64] |= 1 << (at % 64);
                        let slot = self.slots[at];
                        if slot < slot_count {
                            self.reachable[(slot / 32) as usize] |= 1 << (slot % 32);
                        }
                    }
                }
            }
        }
        &self.reachable
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reachable(
        g: &mut VisibilityGraph,
        camera: [i32; 3],
        slots: &[([i32; 3], u32)],
    ) -> Vec<bool> {
        let n = slots.len() as u32;
        for &(k, slot) in slots {
            g.set_slot(k, Some(slot));
        }
        let bits = g.update(camera, n).to_vec();
        (0..n)
            .map(|s| bits[(s / 32) as usize] >> (s % 32) & 1 != 0)
            .collect()
    }

    #[test]
    fn exits_follow_connected_face_pairs() {
        let mut v = SectionVisibility::NONE;
        v.connect(0, 3);
        assert!(v.connected(3, 0));
        assert_eq!(v.exits(1 << 0), 1 << 3);
        assert_eq!(v.exits(1 << 1), 0);
        let mut all = SectionVisibility::NONE;
        all.connect_all(0x3f);
        assert_eq!(all, SectionVisibility::ALL);
        assert_eq!(SectionVisibility::default(), SectionVisibility::ALL);
    }

    #[test]
    fn reached_sections_match_the_search() {
        let mut g = VisibilityGraph::default();
        assert!(g.reached().is_none());
        for y in -3..=3 {
            for z in -3..=3 {
                g.set([2, y, z], SectionVisibility::NONE);
            }
        }
        g.set_slot([3, 0, 0], Some(0));
        g.set_slot([-4, 0, 0], Some(1));
        g.update([0, 0, 0], 2);
        let r = g.reached().unwrap();
        let generation = r.generation;
        assert!(r.contains([0, 0, 0]) && r.contains([1, 0, 0]) && r.contains([-4, 0, 0]));
        assert!(r.contains([2, 0, 0]), "the wall itself is entered");
        assert!(!r.contains([3, 0, 0]), "nothing behind the wall is");
        assert!(r.contains([99, 0, 0]), "outside the box counts as reached");
        g.update([0, 0, 0], 2);
        assert_ne!(g.reached().unwrap().generation, generation);
    }

    #[test]
    fn opaque_shell_hides_what_is_behind_it() {
        let mut g = VisibilityGraph::default();
        for y in -3..=3 {
            for z in -3..=3 {
                g.set([2, y, z], SectionVisibility::NONE);
            }
        }
        let slots = [
            ([1, 0, 0], 0),
            ([2, 0, 0], 1),
            ([3, 0, 0], 2),
            ([-4, 0, 0], 3),
        ];
        assert_eq!(
            reachable(&mut g, [0, 0, 0], &slots),
            [true, true, false, true],
            "the wall itself is drawn, the section behind it is not"
        );
    }

    #[test]
    fn search_never_turns_back_toward_the_camera() {
        let mut g = VisibilityGraph::default();
        let mut corridor = SectionVisibility::NONE;
        corridor.connect(0, 1);
        let mut corner = SectionVisibility::NONE;
        corner.connect(1, 4);
        // A tube leaving along +X that bends into +Z and back toward -X is cut at the turn back.
        for x in 1..=3 {
            g.set([x, 0, 0], corridor);
        }
        g.set([4, 0, 0], corner);
        let mut back = SectionVisibility::NONE;
        back.connect(5, 1);
        g.set([4, 0, 1], back);
        g.set([3, 0, 1], corridor);
        for x in -1..=5 {
            for z in -1..=2 {
                for y in [-1, 1] {
                    g.set([x, y, z], SectionVisibility::NONE);
                }
                if !(0..=1).contains(&z) || x > 4 {
                    g.set([x, 0, z], SectionVisibility::NONE);
                }
            }
        }
        for x in -1..=2 {
            g.set([x, 0, 1], SectionVisibility::NONE);
        }
        let slots = [([4, 0, 0], 0), ([4, 0, 1], 1), ([3, 0, 1], 2)];
        assert_eq!(reachable(&mut g, [0, 0, 0], &slots), [true, true, false]);
    }

    #[test]
    fn unknown_sections_are_open() {
        let mut g = VisibilityGraph::default();
        g.set([5, 5, 5], SectionVisibility::NONE);
        let slots = [([5, 5, 5], 0), ([4, 4, 4], 1)];
        assert_eq!(reachable(&mut g, [0, 0, 0], &slots), [true, true]);
        assert!(!g.needs_update([0, 0, 0]));
        assert!(g.needs_update([0, 1, 0]));
    }
}
