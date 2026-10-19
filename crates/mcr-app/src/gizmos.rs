//! F3+B entity hitboxes and F3+G chunk borders as debug lines.

use mcr_client::ClientState;
use mcr_protocol::types::entity_data::EntityDataValue;
use mcr_render::glam::DVec3;

use crate::outline::WorldLine;

const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const BLUE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];
const YELLOW: [f32; 4] = [1.0, 1.0, 0.0, 1.0];
const CELL_BORDER: [f32; 4] = [0.0, 155.0 / 255.0, 155.0 / 255.0, 1.0];
const MAJOR: [f32; 4] = [0.25, 0.25, 1.0, 1.0];
const NEIGHBOUR: [f32; 4] = [1.0, 0.0, 0.0, 0.5];
const HITBOX_RANGE: f64 = 64.0;
const MAX_HITBOXES: usize = 512;
/// Living entities sync their health as a float at this index.
const HEALTH: u8 = 9;

fn invisible(data: Option<&EntityDataValue>) -> bool {
    matches!(data, Some(EntityDataValue::Byte(f)) if (*f as u8) & 0x20 != 0)
}

fn view_vector(yaw: f32, pitch: f32) -> DVec3 {
    let (yaw, pitch) = (f64::from(yaw).to_radians(), f64::from(pitch).to_radians());
    DVec3::new(
        -yaw.sin() * pitch.cos(),
        -pitch.sin(),
        yaw.cos() * pitch.cos(),
    )
}

/// `EntityHitboxDebugRenderer`: box, eye plane and two-block view arrow for visible entities nearby.
pub fn hitboxes(state: &ClientState, camera: DVec3, first_person: bool, out: &mut Vec<WorldLine>) {
    let data = state.data;
    let own = state.player.entity_id;
    let mut drawn = 0;
    for e in state.entities.iter() {
        if drawn >= MAX_HITBOXES {
            break;
        }
        if invisible(e.data.get(&0)) || e.id == own && first_person {
            continue;
        }
        let pos = DVec3::from(e.pos);
        if pos.distance_squared(camera) > HITBOX_RANGE * HITBOX_RANGE {
            continue;
        }
        drawn += 1;
        let bb = state.entity_box(e);
        let (min, max) = (DVec3::from(bb.min), DVec3::from(bb.max));
        WorldLine::cuboid(min, max, WHITE, 1.0, out);
        let info = u32::try_from(e.kind)
            .ok()
            .and_then(|k| data.entity_type_info(k));
        let eye = info.map_or(0.0, |i| f64::from(i.eye_height));
        if matches!(e.data.get(&HEALTH), Some(EntityDataValue::Float(_))) {
            let y = min.y + eye;
            WorldLine::cuboid(
                DVec3::new(min.x, y - 0.01, min.z),
                DVec3::new(max.x, y + 0.01, max.z),
                RED,
                1.0,
                out,
            );
        }
        let eye_pos = pos + DVec3::new(0.0, eye, 0.0);
        out.push(WorldLine {
            a: eye_pos,
            b: eye_pos + view_vector(e.head_yaw, e.pitch) * 2.0,
            color: BLUE,
            width: 1.0,
        });
    }
}

/// `ChunkBorderRenderer`: the camera's chunk column with a two-block grid and its neighbours' corners.
pub fn chunk_borders(camera: DVec3, min_y: i32, max_y: i32, out: &mut Vec<WorldLine>) {
    let x0 = (camera.x.floor() as i64).div_euclid(16) as f64 * 16.0;
    let z0 = (camera.z.floor() as i64).div_euclid(16) as f64 * 16.0;
    let (y0, y1) = (f64::from(min_y), f64::from(max_y) + 1.0);
    let mut line = |a: DVec3, b: DVec3, color: [f32; 4], width: f32| {
        out.push(WorldLine { a, b, color, width });
    };
    let vertical = |x: f64, z: f64| (DVec3::new(x, y0, z), DVec3::new(x, y1, z));
    for dx in [-16.0, 0.0, 16.0, 32.0] {
        for dz in [-16.0, 0.0, 16.0, 32.0] {
            let (a, b) = vertical(x0 + dx, z0 + dz);
            line(a, b, NEIGHBOUR, 4.0);
        }
    }
    for i in (2..16).step_by(2) {
        let color = if i % 4 == 0 { CELL_BORDER } else { YELLOW };
        let d = f64::from(i);
        for (x, z) in [
            (x0 + d, z0),
            (x0 + d, z0 + 16.0),
            (x0, z0 + d),
            (x0 + 16.0, z0 + d),
        ] {
            let (a, b) = vertical(x, z);
            line(a, b, color, 1.0);
        }
    }
    let ring = |y: f64| {
        [
            (DVec3::new(x0, y, z0), DVec3::new(x0, y, z0 + 16.0)),
            (
                DVec3::new(x0, y, z0 + 16.0),
                DVec3::new(x0 + 16.0, y, z0 + 16.0),
            ),
            (
                DVec3::new(x0 + 16.0, y, z0 + 16.0),
                DVec3::new(x0 + 16.0, y, z0),
            ),
            (DVec3::new(x0 + 16.0, y, z0), DVec3::new(x0, y, z0)),
        ]
    };
    let mut y = min_y;
    while y <= max_y + 1 {
        let color = if y.rem_euclid(8) == 0 {
            CELL_BORDER
        } else {
            YELLOW
        };
        for (a, b) in ring(f64::from(y)) {
            line(a, b, color, 1.0);
        }
        y += 2;
    }
    for dx in [0.0, 16.0] {
        for dz in [0.0, 16.0] {
            let (a, b) = vertical(x0 + dx, z0 + dz);
            line(a, b, MAJOR, 4.0);
        }
    }
    let mut y = min_y;
    while y <= max_y + 1 {
        for (a, b) in ring(f64::from(y)) {
            line(a, b, MAJOR, 4.0);
        }
        y += 16;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_borders_follow_the_camera_chunk() {
        let mut out = Vec::new();
        chunk_borders(DVec3::new(-0.5, 70.0, 17.0), -64, 319, &mut out);
        assert!(out.iter().all(|l| l.a.y >= -64.0 && l.b.y <= 320.0));
        let major: Vec<_> = out
            .iter()
            .filter(|l| l.color == MAJOR && l.a.y != l.b.y)
            .collect();
        assert_eq!(major.len(), 4);
        assert!(major.iter().any(|l| l.a.x == -16.0 && l.a.z == 16.0));
    }
}
