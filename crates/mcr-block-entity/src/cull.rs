use glam::{Vec3, Vec4};

use crate::BlockEntityKind;

/// Stands in for vanilla's infinite-height render boxes without producing `inf * 0` in plane tests.
pub const UNBOUNDED_Y: f32 = 1.0e7;

/// Render box relative to the block's min corner: the unit block inflated by 0.5 as vanilla does for
/// entities, widened for kinds whose model leaves it and unbounded in height for beams.
pub fn render_bounds(kind: BlockEntityKind) -> (Vec3, Vec3) {
    use BlockEntityKind as K;
    match kind {
        K::Beacon | K::EndGateway => (
            Vec3::new(-0.5, -UNBOUNDED_Y, -0.5),
            Vec3::new(1.5, UNBOUNDED_Y, 1.5),
        ),
        K::Piston => (Vec3::splat(-1.0), Vec3::splat(2.0)),
        _ => (Vec3::splat(-0.5), Vec3::splat(1.5)),
    }
}

/// Beams ignore height for the distance test, like vanilla's horizontal-only beacon range.
pub fn beyond_view_distance(kind: BlockEntityKind, rel_center: [f64; 3], limit: f64) -> bool {
    let horizontal = rel_center[0] * rel_center[0] + rel_center[2] * rel_center[2];
    let dist2 = if matches!(kind, BlockEntityKind::Beacon | BlockEntityKind::EndGateway) {
        horizontal
    } else {
        horizontal + rel_center[1] * rel_center[1]
    };
    dist2 > limit * limit
}

/// `planes` are `dot(n, p) + w >= 0` inside, not necessarily normalised; `min`/`max` are camera-relative.
pub fn box_in_frustum(planes: &[Vec4], min: Vec3, max: Vec3) -> bool {
    planes.iter().all(|p| {
        let n = p.truncate();
        let far = Vec3::select(n.cmpge(Vec3::ZERO), max, min);
        n.dot(far) + p.w >= 0.0
    })
}

pub fn block_entity_visible(
    kind: BlockEntityKind,
    pos: [i32; 3],
    camera: [f64; 3],
    render_distance_blocks: f64,
    planes: &[Vec4],
) -> bool {
    let rel = [
        f64::from(pos[0]) + 0.5 - camera[0],
        f64::from(pos[1]) + 0.5 - camera[1],
        f64::from(pos[2]) + 0.5 - camera[2],
    ];
    if beyond_view_distance(kind, rel, kind.view_distance(render_distance_blocks)) {
        return false;
    }
    let origin = Vec3::new(
        rel[0] as f32 - 0.5,
        rel[1] as f32 - 0.5,
        rel[2] as f32 - 0.5,
    );
    let (lo, hi) = render_bounds(kind);
    box_in_frustum(planes, origin + lo, origin + hi)
}

#[cfg(test)]
mod tests {
    use super::*;
    use BlockEntityKind as K;

    fn planes() -> [Vec4; 5] {
        // Camera at origin looking toward -Z with a 90 degree frustum; inside when dot(n, p) + w >= 0.
        [
            Vec4::new(1.0, 0.0, -1.0, 0.0),
            Vec4::new(-1.0, 0.0, -1.0, 0.0),
            Vec4::new(0.0, 1.0, -1.0, 0.0),
            Vec4::new(0.0, -1.0, -1.0, 0.0),
            Vec4::new(0.0, 0.0, -1.0, 0.0),
        ]
    }

    #[test]
    fn aabb_inside_straddling_outside() {
        let p = planes();
        assert!(box_in_frustum(
            &p,
            Vec3::new(-1.0, -1.0, -11.0),
            Vec3::new(1.0, 1.0, -9.0)
        ));
        assert!(box_in_frustum(
            &p,
            Vec3::new(8.0, -1.0, -11.0),
            Vec3::new(12.0, 1.0, -9.0)
        ));
        assert!(!box_in_frustum(
            &p,
            Vec3::new(20.0, -1.0, -11.0),
            Vec3::new(22.0, 1.0, -9.0)
        ));
        assert!(!box_in_frustum(
            &p,
            Vec3::new(-1.0, -1.0, 5.0),
            Vec3::new(1.0, 1.0, 9.0)
        ));
    }

    #[test]
    fn view_distance_rules() {
        assert!(!beyond_view_distance(K::Chest, [64.0, 0.0, 0.0], 64.0));
        assert!(beyond_view_distance(K::Chest, [45.0, 45.0, 45.0], 64.0));
        assert!(beyond_view_distance(K::Chest, [0.0, 65.0, 0.0], 64.0));
        assert!(!beyond_view_distance(K::Beacon, [0.0, 500.0, 30.0], 192.0));
        assert!(beyond_view_distance(K::Beacon, [200.0, 0.0, 0.0], 192.0));
        assert_eq!(K::Beacon.view_distance(192.0), 192.0);
        assert_eq!(K::Piston.view_distance(192.0), 68.0);
    }

    #[test]
    fn beacon_beam_visible_from_far_above_and_below() {
        let p = planes();
        let at = |kind: K, y: f32| {
            let (lo, hi) = render_bounds(kind);
            let origin = Vec3::new(-1.0, y, -10.0);
            box_in_frustum(&p, origin + lo, origin + hi)
        };
        assert!(!at(K::Chest, 300.0) && !at(K::Chest, -300.0));
        assert!(at(K::Beacon, 300.0) && at(K::Beacon, -300.0));
        assert!(block_entity_visible(
            K::Beacon,
            [-1, 300, -10],
            [0.0; 3],
            192.0,
            &p
        ));
    }

    #[test]
    fn infinite_box_stays_finite_in_plane_math() {
        let (lo, hi) = render_bounds(K::Beacon);
        assert!(lo.is_finite() && hi.is_finite());
        assert!(box_in_frustum(
            &planes(),
            lo + Vec3::new(0.0, 0.0, -10.0),
            hi + Vec3::new(0.0, 0.0, -10.0)
        ));
    }

    #[test]
    fn behind_camera_is_culled() {
        assert!(!block_entity_visible(
            K::Chest,
            [0, 0, 10],
            [0.0; 3],
            192.0,
            &planes()
        ));
        assert!(block_entity_visible(
            K::Chest,
            [0, 0, -10],
            [0.0; 3],
            192.0,
            &planes()
        ));
    }
}
