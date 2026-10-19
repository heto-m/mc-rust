//! Client-side vehicle animation inputs: `OldMinecartBehavior`'s rail-following pose.

use glam::DVec3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RailShape {
    NorthSouth,
    EastWest,
    AscendingEast,
    AscendingWest,
    AscendingNorth,
    AscendingSouth,
    SouthEast,
    SouthWest,
    NorthWest,
    NorthEast,
}

impl RailShape {
    /// The `shape` block state property value.
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "north_south" => Self::NorthSouth,
            "east_west" => Self::EastWest,
            "ascending_east" => Self::AscendingEast,
            "ascending_west" => Self::AscendingWest,
            "ascending_north" => Self::AscendingNorth,
            "ascending_south" => Self::AscendingSouth,
            "south_east" => Self::SouthEast,
            "south_west" => Self::SouthWest,
            "north_west" => Self::NorthWest,
            "north_east" => Self::NorthEast,
            _ => return None,
        })
    }

    /// `AbstractMinecart.exits`.
    fn exits(self) -> ([i32; 3], [i32; 3]) {
        const W: [i32; 3] = [-1, 0, 0];
        const E: [i32; 3] = [1, 0, 0];
        const N: [i32; 3] = [0, 0, -1];
        const S: [i32; 3] = [0, 0, 1];
        let below = |v: [i32; 3]| [v[0], v[1] - 1, v[2]];
        match self {
            Self::NorthSouth => (N, S),
            Self::EastWest => (W, E),
            Self::AscendingEast => (below(W), E),
            Self::AscendingWest => (W, below(E)),
            Self::AscendingNorth => (N, below(S)),
            Self::AscendingSouth => (below(N), S),
            Self::SouthEast => (S, E),
            Self::SouthWest => (S, W),
            Self::NorthWest => (N, W),
            Self::NorthEast => (N, E),
        }
    }

    fn is_slope(self) -> bool {
        matches!(
            self,
            Self::AscendingEast | Self::AscendingWest | Self::AscendingNorth | Self::AscendingSouth
        )
    }
}

/// Where `AbstractMinecartRenderer.oldRender` draws a cart standing on rails: the offset from the
/// entity position, and the yaw and pitch (degrees) along the track when it has a direction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RailPose {
    pub offset: [f32; 3],
    pub angles: Option<(f32, f32)>,
}

/// The rail under a cart at `pos`; `rail` gives the shape of a rail block at a block position.
pub fn rail_pose(pos: [f64; 3], rail: impl Fn([i32; 3]) -> Option<RailShape>) -> Option<RailPose> {
    let p = DVec3::from(pos);
    if !p.is_finite() {
        return None;
    }
    let on = rail_pos(p, &rail)?;
    let front = rail_pos_offs(p, 0.3, &rail).unwrap_or(on);
    let back = rail_pos_offs(p, -0.3, &rail).unwrap_or(on);
    let offset = DVec3::new(on.x - p.x, (front.y + back.y) / 2.0 - p.y, on.z - p.z)
        .clamp(DVec3::splat(-2.0), DVec3::splat(2.0))
        .as_vec3();
    let dir = (back - front).normalize_or_zero();
    let angles = (dir != DVec3::ZERO).then(|| {
        (
            dir.z.atan2(dir.x).to_degrees() as f32,
            (dir.y.atan() * 73.0) as f32,
        )
    });
    Some(RailPose {
        offset: offset.into(),
        angles,
    })
}

fn block(x: f64) -> i32 {
    x.floor() as i32
}

/// The rail block a cart at `p` runs on (it may stand one block above it) and its shape.
fn rail_at(
    p: DVec3,
    rail: &impl Fn([i32; 3]) -> Option<RailShape>,
) -> Option<([i32; 3], RailShape)> {
    let (x, mut y, z) = (block(p.x), block(p.y), block(p.z));
    if rail([x, y.saturating_sub(1), z]).is_some() {
        y = y.saturating_sub(1);
    }
    rail([x, y, z]).map(|s| ([x, y, z], s))
}

/// `OldMinecartBehavior.getPosOffs`.
fn rail_pos_offs(
    p: DVec3,
    offs: f64,
    rail: &impl Fn([i32; 3]) -> Option<RailShape>,
) -> Option<DVec3> {
    let ([xt, yt, zt], shape) = rail_at(p, rail)?;
    let mut y = f64::from(yt) + if shape.is_slope() { 1.0 } else { 0.0 };
    let (e0, e1) = shape.exits();
    let (xd, zd) = (f64::from(e1[0] - e0[0]), f64::from(e1[2] - e0[2]));
    let len = xd.hypot(zd);
    let x = p.x + xd / len * offs;
    let z = p.z + zd / len * offs;
    let (dx, dz) = (block(x) - xt, block(z) - zt);
    if e0[1] != 0 && dx == e0[0] && dz == e0[2] {
        y += f64::from(e0[1]);
    } else if e1[1] != 0 && dx == e1[0] && dz == e1[2] {
        y += f64::from(e1[1]);
    }
    rail_pos(DVec3::new(x, y, z), rail)
}

/// `OldMinecartBehavior.getPos`.
fn rail_pos(p: DVec3, rail: &impl Fn([i32; 3]) -> Option<RailShape>) -> Option<DVec3> {
    let ([xt, yt, zt], shape) = rail_at(p, rail)?;
    let (e0, e1) = shape.exits();
    let base = DVec3::new(
        f64::from(xt) + 0.5,
        f64::from(yt) + 0.0625,
        f64::from(zt) + 0.5,
    );
    let p0 = base + DVec3::new(f64::from(e0[0]), f64::from(e0[1]), f64::from(e0[2])) * 0.5;
    let p1 = base + DVec3::new(f64::from(e1[0]), f64::from(e1[1]), f64::from(e1[2])) * 0.5;
    let d = DVec3::new(p1.x - p0.x, (p1.y - p0.y) * 2.0, p1.z - p0.z);
    let progress = if d.x == 0.0 {
        p.z - f64::from(zt)
    } else if d.z == 0.0 {
        p.x - f64::from(xt)
    } else {
        ((p.x - p0.x) * d.x + (p.z - p0.z) * d.z) * 2.0
    };
    let mut out = p0 + d * progress;
    if d.y < 0.0 {
        out.y += 1.0;
    } else if d.y > 0.0 {
        out.y += 0.5;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_track_has_no_pitch_and_follows_the_rail() {
        let rail = |b: [i32; 3]| (b == [0, 0, 0]).then_some(RailShape::EastWest);
        let pose = rail_pose([0.3, 0.0, 0.5], rail).unwrap();
        let (yaw, pitch) = pose.angles.unwrap();
        assert!(pitch.abs() < 1e-4);
        assert!((yaw.abs() - 180.0).abs() < 1e-3);
        assert!((pose.offset[1] - 0.0625).abs() < 1e-4);
    }

    #[test]
    fn slopes_pitch_the_cart() {
        let rail = |b: [i32; 3]| (b == [0, 0, 0]).then_some(RailShape::AscendingEast);
        let (_, pitch) = rail_pose([0.5, 0.5, 0.5], rail).unwrap().angles.unwrap();
        assert!(
            (pitch.abs() - 0.5f32.sqrt().atan() * 73.0).abs() < 0.5,
            "{pitch}"
        );
        assert!(rail_pose([0.5, 5.0, 0.5], rail).is_none());
        assert!(rail_pose([f64::NAN, 0.0, 0.0], rail).is_none());
    }
}
