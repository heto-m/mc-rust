//! Locator bar geometry: where each tracked waypoint sits on the bar and which way to look for it.

use mcr_protocol::play::clientbound::entity::{TrackedWaypoint, WaypointData, WaypointId};

use crate::entity::Entities;

pub const VISIBLE_DEGREES: f64 = 60.0;
pub const BAR_WIDTH: i32 = 173;
pub const MAX_MARKS: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pitch {
    None,
    Up,
    Down,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Mark {
    pub style: String,
    pub color: u32,
    pub distance: f32,
    /// Pixels from the bar's middle dot position.
    pub offset: i32,
    pub pitch: Pitch,
}

#[derive(Debug, Clone, Copy)]
pub struct Camera {
    pub pos: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub fov_y_degrees: f32,
}

fn wrap_degrees(d: f64) -> f64 {
    let d = d.rem_euclid(360.0);
    if d >= 180.0 { d - 360.0 } else { d }
}

/// `ARGB.setBrightness(opaque(hash), 0.9)`: the hue and saturation of the hash colour at fixed brightness.
pub fn default_color(hash: i32) -> u32 {
    let rgb = (hash as u32) & 0x00FF_FFFF;
    let (r, g, b) = (
        (rgb >> 16) as f32,
        ((rgb >> 8) & 255) as f32,
        (rgb & 255) as f32,
    );
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let sat = if max == 0.0 { 0.0 } else { (max - min) / max };
    let v = 0.9f32;
    if sat == 0.0 {
        let c = (v * 255.0).round() as u32;
        return 0xFF00_0000 | (c << 16) | (c << 8) | c;
    }
    let range = max - min;
    let (cr, cg, cb) = ((max - r) / range, (max - g) / range, (max - b) / range);
    let mut hue = if r == max {
        cb - cg
    } else if g == max {
        2.0 + cr - cb
    } else {
        4.0 + cg - cr
    } / 6.0;
    if hue < 0.0 {
        hue += 1.0;
    }
    let segment = (hue - hue.floor()) * 6.0;
    let offset = segment - segment.floor();
    let p = v * (1.0 - sat);
    let q = v * (1.0 - sat * offset);
    let t = v * (1.0 - sat * (1.0 - offset));
    let (r, g, b) = match segment as i32 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    let c = |x: f32| (x * 255.0).round() as u32;
    0xFF00_0000 | (c(r) << 16) | (c(g) << 8) | c(b)
}

/// Java's `String.hashCode` / `UUID.hashCode`, as vanilla feeds them to the colour.
fn id_hash(id: &WaypointId) -> i32 {
    match id {
        WaypointId::Uuid(u) => {
            let (hi, lo) = u.as_u64_pair();
            let x = hi ^ lo;
            ((x >> 32) as i32) ^ (x as i32)
        }
        WaypointId::Name(n) => n
            .encode_utf16()
            .fold(0i32, |h, c| h.wrapping_mul(31).wrapping_add(i32::from(c))),
    }
}

fn project(cam: &Camera, point: [f64; 3]) -> (f64, bool) {
    let (yaw, pitch) = (
        f64::from(cam.yaw).to_radians(),
        f64::from(cam.pitch).to_radians(),
    );
    let fwd = [
        -yaw.sin() * pitch.cos(),
        -pitch.sin(),
        yaw.cos() * pitch.cos(),
    ];
    let up = [
        -yaw.sin() * pitch.sin(),
        pitch.cos(),
        yaw.cos() * pitch.sin(),
    ];
    let d = [
        point[0] - cam.pos[0],
        point[1] - cam.pos[1],
        point[2] - cam.pos[2],
    ];
    let dot = |a: [f64; 3]| a[0] * d[0] + a[1] * d[1] + a[2] * d[2];
    let (f, u) = (dot(fwd), dot(up));
    let tan = (f64::from(cam.fov_y_degrees) / 2.0)
        .to_radians()
        .tan()
        .max(1e-6);
    if f.abs() < 1e-9 {
        return (
            if u >= 0.0 {
                f64::INFINITY
            } else {
                f64::NEG_INFINITY
            },
            f < 0.0,
        );
    }
    (u / f / tan, f < 0.0)
}

fn horizon(cam: &Camera) -> f64 {
    if cam.pitch <= -90.0 {
        return f64::NEG_INFINITY;
    }
    if cam.pitch >= 90.0 {
        return f64::INFINITY;
    }
    let tan = (f64::from(cam.fov_y_degrees) / 2.0)
        .to_radians()
        .tan()
        .max(1e-6);
    f64::from(cam.pitch).to_radians().tan() / tan
}

fn horizon_pitch(cam: &Camera) -> Pitch {
    let h = horizon(cam);
    if h < -1.0 {
        Pitch::Down
    } else if h > 1.0 {
        Pitch::Up
    } else {
        Pitch::None
    }
}

fn yaw_to(cam: &Camera, from: [f64; 3], target: [f64; 3]) -> f64 {
    let (dx, dz) = (from[0] - target[0], from[2] - target[2]);
    let (rx, rz) = (-dz, dx);
    let angle = rz.atan2(rx).to_degrees();
    wrap_degrees(angle - wrap_degrees(f64::from(cam.yaw)))
}

/// Marks visible on the bar, farthest first so nearer dots draw on top. `own` is the local player's
/// UUID and `eye_of` gives the eye position of an entity.
pub fn marks<'a>(
    waypoints: impl Iterator<Item = &'a TrackedWaypoint>,
    cam: &Camera,
    own: uuid::Uuid,
    entities: &Entities,
    eye_of: &dyn Fn(&crate::entity::Entity) -> [f64; 3],
) -> Vec<Mark> {
    let mut out: Vec<Mark> = Vec::new();
    for w in waypoints.take(MAX_MARKS) {
        if matches!(w.id, WaypointId::Uuid(u) if u == own) {
            continue;
        }
        let (angle, distance_sq, pitch) = match &w.data {
            WaypointData::Empty => continue,
            WaypointData::Azimuth { angle } => {
                let a =
                    wrap_degrees(f64::from(angle.to_degrees()) - wrap_degrees(f64::from(cam.yaw)));
                (a, f64::INFINITY, horizon_pitch(cam))
            }
            WaypointData::Chunk { x, z } => {
                let target = [
                    f64::from(*x) * 16.0 + 8.5,
                    cam.pos[1].floor() + 0.5,
                    f64::from(*z) * 16.0 + 8.5,
                ];
                let dx = target[0] - cam.pos[0];
                let dz = target[2] - cam.pos[2];
                (
                    yaw_to(cam, cam.pos, target),
                    dx * dx + dz * dz,
                    horizon_pitch(cam),
                )
            }
            WaypointData::Position { x, y, z } => {
                let center = [
                    f64::from(*x) + 0.5,
                    f64::from(*y) + 0.5,
                    f64::from(*z) + 0.5,
                ];
                let target = match &w.id {
                    WaypointId::Uuid(u) => entities
                        .by_uuid(*u)
                        .filter(|e| {
                            let b = [e.pos[0].floor(), e.pos[1].floor(), e.pos[2].floor()];
                            (b[0] - f64::from(*x)).abs()
                                + (b[1] - f64::from(*y)).abs()
                                + (b[2] - f64::from(*z)).abs()
                                <= 3.0
                        })
                        .map_or(center, eye_of),
                    WaypointId::Name(_) => center,
                };
                let d: f64 = (0..3).map(|i| (target[i] - cam.pos[i]).powi(2)).sum();
                let (ndc_y, behind) = project(cam, target);
                let in_front = if behind { -ndc_y } else { ndc_y };
                let pitch = if in_front < -1.0 {
                    Pitch::Down
                } else if in_front > 1.0 || (behind && ndc_y > 0.0) {
                    Pitch::Up
                } else if behind && ndc_y < 0.0 {
                    Pitch::Down
                } else {
                    Pitch::None
                };
                (yaw_to(cam, cam.pos, target), d, pitch)
            }
        };
        if !(angle > -VISIBLE_DEGREES && angle <= VISIBLE_DEGREES) {
            continue;
        }
        let color = match w.icon.color {
            Some([r, g, b]) => {
                0xFF00_0000 | (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)
            }
            None => default_color(id_hash(&w.id)),
        };
        out.push(Mark {
            style: w.icon.style.to_string(),
            color,
            distance: distance_sq.sqrt() as f32,
            offset: (angle * f64::from(BAR_WIDTH) / 2.0 / VISIBLE_DEGREES).floor() as i32,
            pitch,
        });
    }
    out.sort_by(|a, b| b.distance.total_cmp(&a.distance));
    out
}

/// Which sprite index (into the style's list) a distance picks, as `WaypointStyle.sprite`.
pub fn sprite_index(count: usize, near: f32, far: f32, distance: f32) -> usize {
    if count <= 1 || distance < near {
        return 0;
    }
    if distance >= far {
        return count - 1;
    }
    if count == 3 {
        return 1;
    }
    let t = (distance - near) / (far - near);
    let span = (count - 1) as f32 - 1.0;
    (1.0 + t * span) as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcr_buf::Identifier;
    use mcr_protocol::play::clientbound::entity::WaypointIcon;

    fn icon() -> WaypointIcon {
        WaypointIcon {
            style: Identifier::parse("minecraft:default").unwrap(),
            color: Some([255, 0, 0]),
        }
    }

    fn cam(yaw: f32) -> Camera {
        Camera {
            pos: [0.0, 64.0, 0.0],
            yaw,
            pitch: 0.0,
            fov_y_degrees: 70.0,
        }
    }

    fn at(x: i32, z: i32) -> TrackedWaypoint {
        TrackedWaypoint {
            id: WaypointId::Name("w".into()),
            icon: icon(),
            data: WaypointData::Position { x, y: 64, z },
        }
    }

    fn run(yaw: f32, w: TrackedWaypoint) -> Vec<Mark> {
        marks(
            std::iter::once(&w),
            &cam(yaw),
            uuid::Uuid::nil(),
            &Entities::default(),
            &|e| e.pos,
        )
    }

    #[test]
    fn straight_ahead_sits_in_the_middle() {
        let m = run(0.0, at(0, 20));
        assert_eq!(m.len(), 1);
        assert!(m[0].offset.abs() <= 3, "{m:?}");
        assert_eq!(m[0].pitch, Pitch::None);
        assert_eq!(m[0].color, 0xFFFF_0000);
    }

    #[test]
    fn left_and_right_swap_sides() {
        let east = run(0.0, at(20, 0));
        assert!(east.is_empty() || east[0].offset.abs() >= 80);
        let right = run(0.0, at(-10, 20));
        let left = run(0.0, at(10, 20));
        assert!(
            right[0].offset > 0 && left[0].offset < 0,
            "{right:?} {left:?}"
        );
    }

    #[test]
    fn behind_is_hidden() {
        assert!(run(0.0, at(0, -20)).is_empty());
    }

    #[test]
    fn above_points_up() {
        let mut w = at(0, 5);
        w.data = WaypointData::Position { x: 0, y: 90, z: 5 };
        assert_eq!(run(0.0, w)[0].pitch, Pitch::Up);
    }

    #[test]
    fn own_and_empty_waypoints_are_skipped() {
        let mut w = at(0, 20);
        w.id = WaypointId::Uuid(uuid::Uuid::nil());
        assert!(run(0.0, w).is_empty());
        let mut e = at(0, 20);
        e.data = WaypointData::Empty;
        assert!(run(0.0, e).is_empty());
    }

    #[test]
    fn far_marks_draw_first() {
        let m = marks(
            [at(0, 10), at(0, 50)].iter(),
            &cam(0.0),
            uuid::Uuid::nil(),
            &Entities::default(),
            &|e| e.pos,
        );
        assert!(m[0].distance > m[1].distance);
    }

    #[test]
    fn sprite_choice_follows_distance() {
        assert_eq!(sprite_index(4, 128.0, 332.0, 10.0), 0);
        assert_eq!(sprite_index(4, 128.0, 332.0, 500.0), 3);
        assert_eq!(sprite_index(4, 128.0, 332.0, 200.0), 1);
        assert_eq!(sprite_index(3, 128.0, 332.0, 200.0), 1);
        assert_eq!(sprite_index(1, 128.0, 332.0, 200.0), 0);
    }

    #[test]
    fn hash_colours_are_bright_and_opaque() {
        let c = default_color(0x1234_5678);
        assert_eq!(c >> 24, 0xFF);
        let max = (c >> 16 & 255).max(c >> 8 & 255).max(c & 255);
        assert_eq!(max, (0.9f32 * 255.0).round() as u32);
        assert_eq!(default_color(0), 0xFFE6_E6E6);
    }
}
