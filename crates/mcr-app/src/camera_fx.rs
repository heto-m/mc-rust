//! Camera effects of vanilla's `GameRenderer` and `Camera`: view bobbing, hurt tilt, the nausea and
//! portal warp, the FOV modifier, and the portal, nausea and walk state `LocalPlayer` ticks.

use std::f32::consts::PI;

use mcr_client::ClientState;
use mcr_physics::{Aabb, CollisionWorld, PhysicsData, flag};
use mcr_render::glam::{Mat4, Vec3};

pub const FOV_MODIFIER_MIN: f32 = 0.1;
pub const FOV_MODIFIER_MAX: f32 = 1.5;
const FLUID_FOV: f32 = 6.0 / 7.0;
const PORTAL_IN_STEP: f32 = 0.0125;
const PORTAL_OUT_STEP: f32 = 0.05;
const NAUSEA_BLEND_IN: f32 = 150.0;
const NAUSEA_BLEND_OUT: f32 = 20.0;
const NAUSEA_BLEND_ADVANCE: i32 = 60;
const BOW_FULL_TICKS: f32 = 20.0;

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// `MobEffectInstance.BlendState`: a 0..1 factor that eases in and out with an effect.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct EffectBlend {
    prev: f32,
    cur: f32,
}

impl EffectBlend {
    /// One tick; `remaining` is the effect's duration (-1 infinite), `None` once it is gone.
    pub fn tick(&mut self, remaining: Option<i32>, blend_in: f32, blend_out: f32, advance: i32) {
        let Some(left) = remaining else {
            *self = Self::default();
            return;
        };
        self.prev = self.cur;
        let active = left < 0 || left > advance;
        let target = if active { 1.0 } else { 0.0 };
        if self.cur != target {
            let ticks = if active { blend_in } else { blend_out };
            if ticks <= 0.0 {
                self.cur = target;
            } else {
                let step = 1.0 / ticks;
                self.cur += (target - self.cur).clamp(-step, step);
            }
        }
    }

    pub fn at(&self, partial: f32) -> f32 {
        lerp(self.prev, self.cur, partial)
    }
}

/// `LocalPlayer#handlePortalTransitionEffect`: eases in while inside a nether portal.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PortalEffect {
    prev: f32,
    cur: f32,
}

impl PortalEffect {
    /// One tick; true when the effect starts, which plays `block.portal.trigger`.
    pub fn tick(&mut self, inside: bool) -> bool {
        self.prev = self.cur;
        let mut started = false;
        let step = if inside {
            started = self.cur == 0.0;
            PORTAL_IN_STEP
        } else if self.cur > 0.0 {
            -PORTAL_OUT_STEP
        } else {
            0.0
        };
        self.cur = (self.cur + step).clamp(0.0, 1.0);
        started
    }

    pub fn at(&self, partial: f32) -> f32 {
        lerp(self.prev, self.cur, partial)
    }

    pub fn current(&self) -> f32 {
        self.cur
    }
}

/// `LocalPlayer#tickSpinningEffect`: the angle the warp axis turns by, in degrees.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Spin {
    time: f32,
    speed: f32,
}

impl Spin {
    pub fn tick(&mut self, portal: f32, nausea: f32) {
        if portal > 0.0 || nausea > 0.0 {
            self.speed = (portal * 20.0 + nausea * 7.0) / (portal + nausea);
            self.time = (self.time + self.speed) % 36_000.0;
        } else {
            self.speed = 0.0;
        }
    }

    pub fn angle(&self, partial: f32) -> f32 {
        self.time + partial * self.speed
    }
}

/// `Player#bob` and `Entity#walkDist`, which drive view bobbing.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct WalkBob {
    walk: f32,
    old_walk: f32,
    bob: f32,
    old_bob: f32,
    last_feet: Option<[f64; 3]>,
}

impl WalkBob {
    pub fn tick(&mut self, feet: [f64; 3], vel: [f64; 3], grounded: bool, riding: bool) {
        self.old_walk = self.walk;
        self.old_bob = self.bob;
        let last = self.last_feet.replace(feet).unwrap_or(feet);
        let (dx, dz) = ((feet[0] - last[0]) as f32, (feet[2] - last[2]) as f32);
        let moved = (dx * dx + dz * dz).sqrt();
        if moved.is_finite() {
            self.walk = (self.walk + moved * 0.6) % 1.0e4;
        }
        if riding {
            self.bob = 0.0;
            return;
        }
        let speed = vel[0].hypot(vel[2]) as f32;
        let target = if grounded && speed.is_finite() {
            speed.min(0.1)
        } else {
            0.0
        };
        self.bob += (target - self.bob) * 0.4;
    }

    /// `(backwardsInterpolatedWalkDistance, bob)` for this frame.
    pub fn sample(&self, partial: f32) -> (f32, f32) {
        let delta = self.walk - self.old_walk;
        (
            -(self.walk + delta * partial),
            lerp(self.old_bob, self.bob, partial),
        )
    }
}

/// `GameRenderer#bobView` as a camera-space matrix.
pub fn bob_matrix(walked: f32, amount: f32) -> Mat4 {
    let phase = walked * PI;
    Mat4::from_translation(Vec3::new(
        phase.sin() * amount * 0.5,
        -(phase.cos() * amount).abs(),
        0.0,
    )) * Mat4::from_rotation_z((phase.sin() * amount * 3.0).to_radians())
        * Mat4::from_rotation_x((((phase - 0.2).cos() * amount).abs() * 5.0).to_radians())
}

/// The hurt state `GameRenderer#bobHurt` reads: ticks left (minus the partial tick), duration and
/// the direction of the hit in degrees.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hurt {
    pub time: f32,
    pub duration: f32,
    pub dir: f32,
}

/// `GameRenderer#bobHurt`: the death roll and the damage tilt.
pub fn hurt_matrix(death_ticks: Option<f32>, hurt: Option<Hurt>, tilt_strength: f32) -> Mat4 {
    let mut m = Mat4::IDENTITY;
    if let Some(ticks) = death_ticks {
        let duration = ticks.min(20.0);
        m *= Mat4::from_rotation_z((40.0 - 8000.0 / (duration + 200.0)).to_radians());
    }
    if let Some(h) = hurt.filter(|h| h.time >= 0.0 && h.duration > 0.0) {
        let t = h.time / h.duration;
        let s = (t * t * t * t * PI).sin();
        let tilt = -s * 14.0 * tilt_strength;
        let dir = h.dir.to_radians();
        m *= Mat4::from_rotation_y(-dir)
            * Mat4::from_rotation_z(tilt.to_radians())
            * Mat4::from_rotation_y(dir);
    }
    m
}

/// `GameRenderer#renderLevel`'s spinning skew for nausea and portals; identity at zero intensity.
pub fn warp_matrix(intensity: f32, angle_degrees: f32) -> Mat4 {
    if intensity <= 0.0 || !intensity.is_finite() {
        return Mat4::IDENTITY;
    }
    let mut skew = 5.0 / (intensity * intensity + 5.0) - intensity * 0.04;
    skew *= skew;
    if skew <= 1.0e-4 {
        return Mat4::IDENTITY;
    }
    let axis = Vec3::new(
        0.0,
        std::f32::consts::FRAC_1_SQRT_2,
        std::f32::consts::FRAC_1_SQRT_2,
    );
    let angle = angle_degrees.to_radians();
    Mat4::from_axis_angle(axis, angle)
        * Mat4::from_scale(Vec3::new(1.0 / skew, 1.0, 1.0))
        * Mat4::from_axis_angle(axis, -angle)
}

/// Inputs of `AbstractClientPlayer#getFieldOfViewModifier`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FovInput {
    pub flying: bool,
    /// The movement speed attribute over the abilities' walking speed; `None` when that is zero.
    pub speed_factor: Option<f32>,
    /// Ticks a bow has been drawn.
    pub bow_ticks: Option<f32>,
    pub scoping_first_person: bool,
    pub effect_scale: f32,
}

pub fn fov_target(i: &FovInput) -> f32 {
    let mut modifier = 1.0f32;
    if i.flying {
        modifier *= 1.1;
    }
    if let Some(f) = i.speed_factor.filter(|f| f.is_finite()) {
        modifier *= (f + 1.0) / 2.0;
    }
    if let Some(ticks) = i.bow_ticks {
        let scale = (ticks / BOW_FULL_TICKS).min(1.0);
        modifier *= 1.0 - scale * scale * 0.15;
    } else if i.scoping_first_person {
        return FOV_MODIFIER_MIN;
    }
    lerp(1.0, modifier, i.effect_scale)
}

/// `Camera#tickFov`: eases half way to the target and clamps.
pub fn step_fov(current: f32, target: f32) -> f32 {
    let target = if target.is_finite() { target } else { 1.0 };
    (current + (target - current) * 0.5).clamp(FOV_MODIFIER_MIN, FOV_MODIFIER_MAX)
}

/// `Camera#modifyFovBasedOnDeathOrFluid`, for the world and the hand.
pub fn death_or_fluid_fov(
    mut fov: f32,
    death_ticks: Option<f32>,
    in_fluid: bool,
    effect_scale: f32,
) -> f32 {
    if let Some(ticks) = death_ticks {
        let duration = ticks.min(20.0);
        fov /= (1.0 - 500.0 / (duration + 500.0)) * 2.0 + 1.0;
    }
    if in_fluid {
        fov *= lerp(1.0, FLUID_FOV, effect_scale);
    }
    fov
}

/// All camera-effect state ticked with the game.
#[derive(Debug, Clone, Default)]
pub struct CameraFx {
    pub nausea: EffectBlend,
    pub portal: PortalEffect,
    pub spin: Spin,
    pub walk: WalkBob,
}

impl CameraFx {
    /// One tick; returns true when the portal effect starts.
    pub fn tick(&mut self, state: &ClientState, nausea_id: Option<i32>) -> bool {
        let player = &state.player;
        let nausea = nausea_id
            .and_then(|id| player.effects.get(&id))
            .map(|e| e.duration_ticks);
        self.nausea.tick(
            nausea,
            NAUSEA_BLEND_IN,
            NAUSEA_BLEND_OUT,
            NAUSEA_BLEND_ADVANCE,
        );
        let inside = !player.dead
            && player.vehicle.is_none()
            && inside_nether_portal(&state.chunks, &player.physics.bounding_box());
        let started = self.portal.tick(inside);
        self.spin.tick(self.portal.current(), self.nausea.at(1.0));
        let p = &player.physics;
        let grounded = p.on_ground && !player.dead && !p.swimming;
        self.walk
            .tick(player.pos(), p.vel, grounded, player.vehicle.is_some());
        started
    }

    /// `max(portal, nausea) * screenEffectScale²`, the warp strength.
    pub fn spinning_intensity(&self, partial: f32, screen_effect_scale: f32) -> f32 {
        let s = screen_effect_scale.clamp(0.0, 1.0);
        self.portal.at(partial).max(self.nausea.at(partial)) * s * s
    }
}

fn block_name(state: u32) -> &'static str {
    mcr_data::v26_3()
        .state_block_name(state)
        .unwrap_or("")
        .trim_start_matches("minecraft:")
}

/// `NetherPortalBlock#entityInside` reached by `Entity#checkInsideBlocks` for the player's box.
pub fn inside_nether_portal<W: CollisionWorld + ?Sized>(world: &W, aabb: &Aabb) -> bool {
    let b = aabb.inflate(-1.0e-5);
    let lo = b.min.map(|v| v.floor() as i32);
    let hi = b.max.map(|v| v.floor() as i32);
    if (0..3).any(|a| hi[a] - lo[a] > 4) {
        return false;
    }
    for x in lo[0]..=hi[0] {
        for y in lo[1]..=hi[1] {
            for z in lo[2]..=hi[2] {
                if block_name(world.block_state(x, y, z)) == "nether_portal" {
                    return true;
                }
            }
        }
    }
    false
}

/// `BlockState#isViewBlocking` for the in-block overlay: suffocating blocks, a full snow stack, and
/// stairs or slabs whose shape meets the near plane.
fn view_blocking(state: u32, pos: [i32; 3], near: &Aabb) -> bool {
    let data = PhysicsData::v26_3();
    let info = data.state(state);
    let name = block_name(state);
    if matches!(
        name,
        "air" | "cave_air" | "void_air" | "barrier" | "light" | "structure_void" | "moving_piston"
    ) {
        return false;
    }
    if info.has(flag::SUFFOCATING) {
        return true;
    }
    if name == "snow" {
        return mcr_data::v26_3()
            .state_properties(state)
            .any(|(k, v)| k == "layers" && v == "8");
    }
    if name.ends_with("_stairs") || name.ends_with("_slab") {
        let off = pos.map(f64::from);
        return data.boxes(info).iter().any(|b| {
            Aabb::new(
                [b[0] + off[0], b[1] + off[1], b[2] + off[2]],
                [b[3] + off[0], b[4] + off[1], b[5] + off[2]],
            )
            .intersects(near)
        });
    }
    false
}

/// `LevelExtractor#getViewBlockingState`: the block whose texture covers the screen, tested at eight
/// points around the eye.
pub fn view_blocking_state<W: CollisionWorld + ?Sized>(
    world: &W,
    feet: [f64; 3],
    eye_y: f64,
    width: f64,
    scale: f64,
    near: &Aabb,
) -> Option<u32> {
    (0..8).find_map(|i| {
        let side = |bit: i32| f64::from((i >> bit) & 1) - 0.5;
        let pos = [
            (feet[0] + side(0) * width * 0.8).floor() as i32,
            (eye_y + side(1) * 0.1 * scale).floor() as i32,
            (feet[2] + side(2) * width * 0.8).floor() as i32,
        ];
        let state = world.block_state(pos[0], pos[1], pos[2]);
        view_blocking(state, pos, near).then_some(state)
    })
}

/// The box around the camera's near plane (0.05 blocks ahead), in world space.
pub fn near_plane_box(eye: [f64; 3], forward: Vec3, up: Vec3, fov_y: f32, aspect: f32) -> Aabb {
    let near = 0.05f32;
    let half_h = near * (fov_y * 0.5).tan();
    let half_w = half_h * aspect;
    let right = forward.cross(up).normalize_or_zero();
    let centre = forward * near;
    let mut lo = [f64::MAX; 3];
    let mut hi = [f64::MIN; 3];
    for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        let p = centre + right * (half_w * sx) + up * (half_h * sy);
        for a in 0..3 {
            let v = eye[a] + f64::from(p[a]);
            lo[a] = lo[a].min(v);
            hi[a] = hi[a].max(v);
        }
    }
    Aabb::new(lo, hi)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> FovInput {
        FovInput {
            flying: false,
            speed_factor: Some(1.0),
            bow_ticks: None,
            scoping_first_person: false,
            effect_scale: 1.0,
        }
    }

    #[test]
    fn fov_modifier_clamps_and_eases() {
        let mut fov = 1.0;
        for _ in 0..20 {
            fov = step_fov(fov, 5.0);
        }
        assert_eq!(fov, FOV_MODIFIER_MAX);
        for _ in 0..20 {
            fov = step_fov(fov, 0.0);
        }
        assert_eq!(fov, FOV_MODIFIER_MIN);
        assert_eq!(step_fov(1.0, f32::NAN), 1.0);
    }

    #[test]
    fn bow_draw_zooms_up_to_fifteen_percent() {
        assert_eq!(fov_target(&input()), 1.0);
        let half = fov_target(&FovInput {
            bow_ticks: Some(10.0),
            ..input()
        });
        assert!((half - (1.0 - 0.25 * 0.15)).abs() < 1e-6);
        let full = fov_target(&FovInput {
            bow_ticks: Some(40.0),
            ..input()
        });
        assert!((full - 0.85).abs() < 1e-6);
        let scaled = fov_target(&FovInput {
            bow_ticks: Some(40.0),
            effect_scale: 0.0,
            ..input()
        });
        assert_eq!(scaled, 1.0);
    }

    #[test]
    fn scoping_ignores_the_effect_scale_but_not_a_drawn_bow() {
        let scope = FovInput {
            scoping_first_person: true,
            effect_scale: 0.0,
            ..input()
        };
        assert_eq!(fov_target(&scope), FOV_MODIFIER_MIN);
        let sprint = FovInput {
            speed_factor: Some(1.3),
            flying: true,
            ..input()
        };
        assert!((fov_target(&sprint) - 1.1 * 1.15).abs() < 1e-5);
    }

    #[test]
    fn fluid_and_death_narrow_the_fov() {
        assert!((death_or_fluid_fov(70.0, None, true, 1.0) - 60.0).abs() < 1e-4);
        assert_eq!(death_or_fluid_fov(70.0, None, true, 0.0), 70.0);
        let dead = death_or_fluid_fov(70.0, Some(20.0), false, 1.0);
        assert!(dead < 70.0 && dead > 60.0);
        assert_eq!(death_or_fluid_fov(70.0, Some(0.0), false, 1.0), 70.0);
    }

    #[test]
    fn portal_state_eases_in_slowly_out_fast_and_triggers_once() {
        let mut p = PortalEffect::default();
        assert!(p.tick(true));
        for _ in 0..10 {
            assert!(!p.tick(true));
        }
        assert!((p.current() - 11.0 * 0.0125).abs() < 1e-5);
        for _ in 0..3 {
            p.tick(false);
        }
        assert_eq!(p.current(), 0.0);
        assert!(p.tick(true));
        for _ in 0..200 {
            p.tick(true);
        }
        assert_eq!(p.current(), 1.0);
    }

    #[test]
    fn nausea_blends_in_and_out_before_it_ends() {
        let mut b = EffectBlend::default();
        for _ in 0..75 {
            b.tick(
                Some(600),
                NAUSEA_BLEND_IN,
                NAUSEA_BLEND_OUT,
                NAUSEA_BLEND_ADVANCE,
            );
        }
        assert!((b.at(1.0) - 0.5).abs() < 1e-3);
        for _ in 0..76 {
            b.tick(
                Some(-1),
                NAUSEA_BLEND_IN,
                NAUSEA_BLEND_OUT,
                NAUSEA_BLEND_ADVANCE,
            );
        }
        assert_eq!(b.at(1.0), 1.0);
        for _ in 0..10 {
            b.tick(
                Some(50),
                NAUSEA_BLEND_IN,
                NAUSEA_BLEND_OUT,
                NAUSEA_BLEND_ADVANCE,
            );
        }
        assert!((b.at(1.0) - 0.5).abs() < 1e-3);
        b.tick(
            None,
            NAUSEA_BLEND_IN,
            NAUSEA_BLEND_OUT,
            NAUSEA_BLEND_ADVANCE,
        );
        assert_eq!(b.at(0.5), 0.0);
    }

    #[test]
    fn spin_speed_mixes_portal_and_nausea() {
        let mut s = Spin::default();
        s.tick(0.0, 1.0);
        assert_eq!(s.angle(0.0), 7.0);
        s.tick(1.0, 0.0);
        assert_eq!(s.angle(0.0), 27.0);
        assert_eq!(s.angle(0.5), 37.0);
        s.tick(0.0, 0.0);
        assert_eq!(s.angle(1.0), 27.0);
    }

    #[test]
    fn warp_is_identity_without_intensity_and_stretches_with_it() {
        assert_eq!(warp_matrix(0.0, 30.0), Mat4::IDENTITY);
        let m = warp_matrix(1.0, 0.0);
        let skew = (5.0f32 / 6.0 - 0.04).powi(2);
        assert!((m.x_axis.x - 1.0 / skew).abs() < 1e-4);
        assert!((m.y_axis.y - 1.0).abs() < 1e-5);
    }

    #[test]
    fn hurt_tilt_peaks_mid_animation_and_bob_follows_walk() {
        assert_eq!(hurt_matrix(None, None, 1.0), Mat4::IDENTITY);
        let hit = hurt_matrix(
            None,
            Some(Hurt {
                time: 8.4,
                duration: 10.0,
                dir: 0.0,
            }),
            1.0,
        );
        assert!(hit.x_axis.y.abs() > 0.2);
        let none = hurt_matrix(
            None,
            Some(Hurt {
                time: 8.4,
                duration: 10.0,
                dir: 0.0,
            }),
            0.0,
        );
        assert!(none.abs_diff_eq(Mat4::IDENTITY, 1e-6));
        let bob = bob_matrix(0.5, 0.1);
        assert!((bob.w_axis.x - 0.05).abs() < 1e-5);
        assert!(bob_matrix(0.3, 0.0).abs_diff_eq(Mat4::IDENTITY, 1e-6));
    }

    struct World(Vec<([i32; 3], &'static str)>);

    impl CollisionWorld for World {
        fn block_state(&self, x: i32, y: i32, z: i32) -> u32 {
            let data = mcr_data::v26_3();
            self.0
                .iter()
                .find(|(p, _)| *p == [x, y, z])
                .and_then(|(_, name)| {
                    let (first, _) = data.block_info(data.block_by_name(name)?)?.states;
                    Some(first)
                })
                .unwrap_or(0)
        }
    }

    #[test]
    fn view_blocking_needs_a_suffocating_block_at_the_eye() {
        let feet = [0.5, -60.0, 0.5];
        let eye = -58.38;
        let near = near_plane_box(
            [0.5, eye, 0.5],
            Vec3::NEG_Z,
            Vec3::Y,
            70f32.to_radians(),
            1.6,
        );
        let stone = World(vec![([0, -59, 0], "minecraft:stone")]);
        assert!(view_blocking_state(&stone, feet, eye, 0.6, 1.0, &near).is_some());
        let glass = World(vec![([0, -59, 0], "minecraft:glass")]);
        assert!(view_blocking_state(&glass, feet, eye, 0.6, 1.0, &near).is_none());
        let below = World(vec![([0, -60, 0], "minecraft:stone")]);
        assert!(view_blocking_state(&below, feet, eye, 0.6, 1.0, &near).is_none());
        let portal = World(vec![([0, -59, 0], "minecraft:nether_portal")]);
        let body = Aabb::new([0.2, -60.0, 0.2], [0.8, -58.2, 0.8]);
        assert!(inside_nether_portal(&portal, &body));
        assert!(!inside_nether_portal(&below, &body));
    }

    #[test]
    fn walk_bob_only_on_the_ground() {
        let mut w = WalkBob::default();
        w.tick([0.0; 3], [0.0; 3], true, false);
        w.tick([0.2, 0.0, 0.0], [0.2, 0.0, 0.0], true, false);
        let (walked, amount) = w.sample(0.0);
        assert!((walked + 0.12).abs() < 1e-5);
        assert!((w.sample(1.0).0 + 0.24).abs() < 1e-5);
        assert!((w.sample(1.0).1 - 0.04).abs() < 1e-5);
        assert_eq!(amount, 0.0);
        w.tick([0.4, 0.0, 0.0], [0.2, 0.0, 0.0], false, false);
        assert!(w.sample(1.0).1 < 0.04);
        w.tick([0.6, 0.0, 0.0], [0.2, 0.0, 0.0], true, true);
        assert_eq!(w.sample(1.0).1, 0.0);
    }
}
