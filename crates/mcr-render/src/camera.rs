use glam::{DVec3, IVec3, Mat4, Vec3, Vec4};

#[derive(Debug, Clone, Copy)]
pub struct Camera {
    pub position: DVec3,
    pub yaw: f32,
    pub pitch: f32,
    pub fov_y: f32,
    pub near: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            position: DVec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            fov_y: 70f32.to_radians(),
            near: 0.05,
        }
    }
}

impl Camera {
    /// Yaw 0 looks toward -Z; positive yaw turns left.
    pub fn forward(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        Vec3::new(-sy * cp, sp, -cy * cp)
    }

    /// Derivative of `forward` with respect to pitch, so it stays defined when looking straight up or down.
    pub fn up(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        Vec3::new(sy * sp, cp, cy * sp)
    }

    pub fn right(&self) -> Vec3 {
        self.forward().cross(self.up()).normalize_or_zero()
    }

    pub fn view_rotation(&self) -> Mat4 {
        glam::camera::rh::view::look_to_mat4(Vec3::ZERO, self.forward(), self.up())
    }

    pub fn projection(&self, aspect: f32) -> Mat4 {
        glam::camera::rh::proj::directx::perspective_infinite_reverse(self.fov_y, aspect, self.near)
    }

    /// Rotation-only view; world geometry is supplied camera-relative.
    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        self.projection(aspect) * self.view_rotation()
    }

    pub fn block_pos(&self) -> IVec3 {
        self.position.floor().as_ivec3()
    }

    pub fn frac(&self) -> Vec3 {
        (self.position - self.position.floor()).as_vec3()
    }

    pub fn frustum(&self, aspect: f32) -> Frustum {
        Frustum::from_view_proj(self.view_proj(aspect))
    }
}

/// Left, right, bottom, top, near planes in camera-relative space; inside when `dot(n, p) + d >= 0`.
#[derive(Debug, Clone, Copy)]
pub struct Frustum(pub [Vec4; 5]);

impl Frustum {
    pub fn from_view_proj(m: Mat4) -> Self {
        let row = |i: usize| Vec4::new(m.x_axis[i], m.y_axis[i], m.z_axis[i], m.w_axis[i]);
        let (r0, r1, r2, r3) = (row(0), row(1), row(2), row(3));
        let planes =
            [r3 + r0, r3 - r0, r3 + r1, r3 - r1, r3 - r2].map(|p| p / p.truncate().length());
        Self(planes)
    }

    pub fn intersects_aabb(&self, min: Vec3, max: Vec3) -> bool {
        self.0.iter().all(|p| {
            let n = p.truncate();
            let far = Vec3::select(n.cmpge(Vec3::ZERO), max, min);
            n.dot(far) + p.w >= 0.0
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forward_defaults_to_negative_z() {
        let c = Camera::default();
        assert!((c.forward() - Vec3::NEG_Z).length() < 1e-6);
        assert!((c.right() - Vec3::X).length() < 1e-6);
    }

    #[test]
    fn straight_down_keeps_orientation() {
        let down = Camera {
            yaw: 0.7,
            pitch: -std::f32::consts::FRAC_PI_2,
            ..Camera::default()
        };
        let view = down.view_rotation();
        let map = |v: Vec3| view.transform_vector3(v);
        assert!((map(down.forward()) - Vec3::NEG_Z).length() < 1e-5);
        assert!((map(down.up()) - Vec3::Y).length() < 1e-5);
        assert!((map(down.right()) - Vec3::X).length() < 1e-5);
        let level = Camera { pitch: 0.0, ..down };
        assert!((down.right() - level.right()).length() < 1e-5);
    }

    #[test]
    fn reverse_z_depth() {
        let c = Camera::default();
        let vp = c.view_proj(1.0);
        let near = vp * Vec4::new(0.0, 0.0, -c.near, 1.0);
        let far = vp * Vec4::new(0.0, 0.0, -1.0e6, 1.0);
        assert!((near.z / near.w - 1.0).abs() < 1e-5);
        assert!(far.z / far.w < 1e-6 && far.z / far.w > 0.0);
    }

    #[test]
    fn frustum_culls_behind_and_sides() {
        let f = Camera::default().frustum(16.0 / 9.0);
        let cube = |c: Vec3| f.intersects_aabb(c - Vec3::splat(0.5), c + Vec3::splat(0.5));
        assert!(cube(Vec3::new(0.0, 0.0, -10.0)));
        assert!(!cube(Vec3::new(0.0, 0.0, 10.0)));
        assert!(!cube(Vec3::new(100.0, 0.0, -10.0)));
        assert!(!cube(Vec3::new(0.0, 100.0, -10.0)));
        assert!(cube(Vec3::new(0.0, 0.0, -1.0e5)));
    }

    #[test]
    fn position_split_is_exact_far_away() {
        let c = Camera {
            position: DVec3::new(29_999_984.25, 70.5, -29_999_984.75),
            ..Camera::default()
        };
        assert_eq!(c.block_pos(), IVec3::new(29_999_984, 70, -29_999_985));
        assert_eq!(c.frac(), Vec3::new(0.25, 0.5, 0.25));
    }
}
