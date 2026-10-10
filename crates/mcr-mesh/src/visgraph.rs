//! Face-to-face connectivity of a section through cells that are not opaque full cubes.

use mcr_render::SectionVisibility;

use crate::cache::ATTR_SOLID_RENDER;
use crate::input::{PADDED, pidx};

/// Fewer opaque cells than this cannot separate any two faces of a 16^3 section.
const MIN_BLOCKING: usize = 256;

fn local(i: usize) -> (usize, usize, usize) {
    (i & 15, i >> 8, i >> 4 & 15)
}

fn boundary_faces(i: usize) -> u8 {
    let (x, y, z) = local(i);
    let mut f = 0;
    f |= u8::from(x == 15);
    f |= u8::from(x == 0) << 1;
    f |= u8::from(y == 15) << 2;
    f |= u8::from(y == 0) << 3;
    f |= u8::from(z == 15) << 4;
    f |= u8::from(z == 0) << 5;
    f
}

/// `meta` is the padded per-cell meta array the mesher builds; `stack` is reused scratch.
pub(crate) fn section_visibility(meta: &[u32; PADDED], stack: &mut Vec<u16>) -> SectionVisibility {
    let mut blocked = [0u64; 64];
    let mut opaque = 0;
    for i in 0..4096 {
        let (x, y, z) = local(i);
        if meta[pidx(x as i32, y as i32, z as i32)] & u32::from(ATTR_SOLID_RENDER) != 0 {
            blocked[i >> 6] |= 1 << (i & 63);
            opaque += 1;
        }
    }
    if opaque < MIN_BLOCKING {
        return SectionVisibility::ALL;
    }
    let mut vis = SectionVisibility::NONE;
    if opaque == 4096 {
        return vis;
    }
    let mut seen = blocked;
    for seed in 0..4096 {
        if seen[seed >> 6] >> (seed & 63) & 1 != 0 || boundary_faces(seed) == 0 {
            continue;
        }
        seen[seed >> 6] |= 1 << (seed & 63);
        stack.clear();
        stack.push(seed as u16);
        let mut faces = 0u8;
        while let Some(i) = stack.pop() {
            let i = usize::from(i);
            faces |= boundary_faces(i);
            let (x, y, z) = local(i);
            let mut visit = |n: usize| {
                if seen[n >> 6] >> (n & 63) & 1 == 0 {
                    seen[n >> 6] |= 1 << (n & 63);
                    stack.push(n as u16);
                }
            };
            if x < 15 {
                visit(i + 1);
            }
            if x > 0 {
                visit(i - 1);
            }
            if z < 15 {
                visit(i + 16);
            }
            if z > 0 {
                visit(i - 16);
            }
            if y < 15 {
                visit(i + 256);
            }
            if y > 0 {
                visit(i - 256);
            }
        }
        vis.connect_all(faces);
    }
    vis
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta_with(solid: impl Fn(i32, i32, i32) -> bool) -> Box<[u32; PADDED]> {
        let mut meta = Box::new([0u32; PADDED]);
        for y in 0..16 {
            for z in 0..16 {
                for x in 0..16 {
                    if solid(x, y, z) {
                        meta[pidx(x, y, z)] = u32::from(ATTR_SOLID_RENDER);
                    }
                }
            }
        }
        meta
    }

    #[test]
    fn open_and_full_sections() {
        let mut stack = Vec::new();
        assert_eq!(
            section_visibility(&meta_with(|_, _, _| false), &mut stack),
            SectionVisibility::ALL
        );
        assert_eq!(
            section_visibility(&meta_with(|_, _, _| true), &mut stack),
            SectionVisibility::NONE
        );
    }

    #[test]
    fn floor_separates_top_from_bottom() {
        let mut stack = Vec::new();
        let v = section_visibility(&meta_with(|_, y, _| y == 7), &mut stack);
        assert!(!v.connected(2, 3));
        assert!(v.connected(2, 0) && v.connected(3, 5));
    }

    #[test]
    fn tunnel_connects_only_its_ends() {
        let mut stack = Vec::new();
        let v = section_visibility(
            &meta_with(|_, y, z| !((6..9).contains(&y) && (6..9).contains(&z))),
            &mut stack,
        );
        assert!(v.connected(0, 1));
        for f in 2..6 {
            assert!(!v.connected(0, f) && !v.connected(1, f));
        }
        assert!(!v.connected(2, 3));
    }
}
