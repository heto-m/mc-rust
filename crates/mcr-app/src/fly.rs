use mcr_render::Camera;
use mcr_render::glam::{DVec3, Vec3};

const LOOK_SENSITIVITY: f32 = 0.0022;
const MAX_PITCH: f32 = 1.553;

#[derive(Debug)]
pub struct FlyController {
    pub forward: bool,
    pub back: bool,
    pub left: bool,
    pub right: bool,
    pub up: bool,
    pub down: bool,
    pub sprint: bool,
    pub speed: f64,
}

impl Default for FlyController {
    fn default() -> Self {
        Self {
            forward: false,
            back: false,
            left: false,
            right: false,
            up: false,
            down: false,
            sprint: false,
            speed: 20.0,
        }
    }
}

fn axis(positive: bool, negative: bool) -> f32 {
    f32::from(u8::from(positive)) - f32::from(u8::from(negative))
}

impl FlyController {
    pub fn look(&self, camera: &mut Camera, dx: f64, dy: f64) {
        camera.yaw -= dx as f32 * LOOK_SENSITIVITY;
        camera.pitch = (camera.pitch - dy as f32 * LOOK_SENSITIVITY).clamp(-MAX_PITCH, MAX_PITCH);
    }

    pub fn scale_speed(&mut self, steps: f64) {
        self.speed = (self.speed * 1.25f64.powf(steps)).clamp(1.0, 2000.0);
    }

    pub fn update(&self, camera: &mut Camera, dt: f32) {
        let dir = camera.forward() * axis(self.forward, self.back)
            + camera.right() * axis(self.right, self.left)
            + Vec3::Y * axis(self.up, self.down);
        let speed = self.speed * if self.sprint { 5.0 } else { 1.0 };
        camera.position += DVec3::from(dir.normalize_or_zero()) * speed * f64::from(dt);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_along_view_direction() {
        let mut cam = Camera::default();
        let fly = FlyController {
            forward: true,
            ..FlyController::default()
        };
        fly.update(&mut cam, 0.5);
        assert!((cam.position - DVec3::new(0.0, 0.0, -10.0)).length() < 1e-5);
    }

    #[test]
    fn opposing_keys_cancel_and_pitch_clamps() {
        let mut cam = Camera::default();
        let fly = FlyController {
            left: true,
            right: true,
            ..FlyController::default()
        };
        fly.update(&mut cam, 1.0);
        assert_eq!(cam.position, DVec3::ZERO);
        fly.look(&mut cam, 0.0, -1.0e6);
        assert!(cam.pitch <= MAX_PITCH);
    }
}
