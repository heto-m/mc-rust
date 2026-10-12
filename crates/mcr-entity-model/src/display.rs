use std::cell::Cell;

use glam::{Mat4, Quat, Vec3};
use mcr_protocol::types::Component;
use mcr_protocol::types::entity_data::EntityDataValue;

use crate::idx;
use crate::view::EntityData;

const MAX_COORD: f32 = 64.0;
const MAX_DURATION: i32 = 1 << 20;
const NEVER: i64 = i64::MIN / 2;

#[derive(Debug, Clone, Copy, PartialEq)]
struct Transform {
    translation: Vec3,
    left: Quat,
    scale: Vec3,
    right: Quat,
}

impl Transform {
    const IDENTITY: Self = Self {
        translation: Vec3::ZERO,
        left: Quat::IDENTITY,
        scale: Vec3::ONE,
        right: Quat::IDENTITY,
    };

    fn read(data: &dyn EntityData) -> Self {
        let vec = |i: u8, d: Vec3| match data.get(i) {
            Some(EntityDataValue::Vector3(v)) => Vec3::new(v.x, v.y, v.z),
            _ => d,
        };
        let quat = |i: u8| match data.get(i) {
            Some(EntityDataValue::Quaternion(q)) => {
                let q = Quat::from_xyzw(q[0], q[1], q[2], q[3]);
                if q.is_finite() && q.length_squared() > 1e-12 {
                    q.normalize()
                } else {
                    Quat::IDENTITY
                }
            }
            _ => Quat::IDENTITY,
        };
        let clamp = |v: Vec3| {
            Vec3::new(
                clamp_finite(v.x, MAX_COORD),
                clamp_finite(v.y, MAX_COORD),
                clamp_finite(v.z, MAX_COORD),
            )
        };
        Self {
            translation: clamp(vec(idx::DISPLAY_TRANSLATION, Vec3::ZERO)),
            left: quat(idx::DISPLAY_LEFT_ROTATION),
            scale: clamp(vec(idx::DISPLAY_SCALE, Vec3::ONE)),
            right: quat(idx::DISPLAY_RIGHT_ROTATION),
        }
    }

    fn slerp(&self, to: &Self, t: f32) -> Self {
        if t >= 1.0 {
            return *to;
        }
        Self {
            translation: self.translation.lerp(to.translation, t),
            left: self.left.slerp(to.left, t),
            scale: self.scale.lerp(to.scale, t),
            right: self.right.slerp(to.right, t),
        }
    }

    fn matrix(&self) -> Mat4 {
        Mat4::from_translation(self.translation)
            * Mat4::from_quat(self.left)
            * Mat4::from_scale(self.scale)
            * Mat4::from_quat(self.right)
    }
}

fn clamp_finite(v: f32, limit: f32) -> f32 {
    if v.is_finite() {
        v.clamp(-limit, limit)
    } else {
        0.0
    }
}

/// `Mth.lerpInt`.
fn lerp_int(t: f32, a: i32, b: i32) -> i32 {
    a.wrapping_add((t * b.wrapping_sub(a) as f32).floor() as i32)
}

fn lerp_argb(t: f32, a: u32, b: u32) -> u32 {
    (0..4).fold(0, |out, i| {
        let shift = i * 8;
        let from = ((a >> shift) & 0xFF) as i32;
        let to = ((b >> shift) & 0xFF) as i32;
        out | (lerp_int(t, from, to).clamp(0, 255) as u32) << shift
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct TextParams {
    opacity: i32,
    background: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Params {
    transform: Transform,
    text: TextParams,
}

#[derive(Debug, Clone, PartialEq)]
struct Seen {
    delay: i32,
    duration: i32,
    transform: Transform,
    text: Option<Component>,
    line_width: i32,
    text_params: TextParams,
    flags: i8,
}

/// What a frame needs from a display entity: the interpolated transformation and text colours.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DisplaySample {
    pub transform: Mat4,
    /// `TextRenderState.textOpacity` as the byte vanilla casts it to.
    pub opacity: u8,
    /// Interpolated ARGB background; the default-background flag is resolved by the caller.
    pub background: u32,
    /// Changes whenever the text or its line width does.
    pub text_rev: u64,
}

/// Client-side `Display` state: change detection, interpolation start and duration.
#[derive(Debug, Clone)]
pub struct DisplayTrack {
    ticks: i64,
    start_tick: i64,
    duration: i32,
    last_progress: Cell<f32>,
    seen: Option<Seen>,
    from: Params,
    to: Params,
    live: bool,
    text_rev: u64,
}

impl Default for DisplayTrack {
    fn default() -> Self {
        let params = Params {
            transform: Transform::IDENTITY,
            text: TextParams {
                opacity: -1,
                background: 0x4000_0000,
            },
        };
        Self {
            ticks: 0,
            start_tick: NEVER,
            duration: 0,
            last_progress: Cell::new(0.0),
            seen: None,
            from: params,
            to: params,
            live: false,
            text_rev: 0,
        }
    }
}

fn read_seen(data: &dyn EntityData) -> Seen {
    let int = |i: u8, d: i32| match data.get(i) {
        Some(EntityDataValue::Int(v)) => *v,
        _ => d,
    };
    Seen {
        delay: int(idx::DISPLAY_INTERPOLATION_DELAY, 0),
        duration: int(idx::DISPLAY_INTERPOLATION_DURATION, 0).clamp(0, MAX_DURATION),
        transform: Transform::read(data),
        text: match data.get(idx::TEXT_DISPLAY_TEXT) {
            Some(EntityDataValue::Component(c)) => Some(c.clone()),
            _ => None,
        },
        line_width: int(idx::TEXT_DISPLAY_LINE_WIDTH, 200),
        text_params: TextParams {
            opacity: match data.get(idx::TEXT_DISPLAY_OPACITY) {
                Some(EntityDataValue::Byte(v)) => i32::from(*v),
                _ => -1,
            },
            background: int(idx::TEXT_DISPLAY_BACKGROUND, 0x4000_0000) as u32,
        },
        flags: match data.get(idx::TEXT_DISPLAY_FLAGS) {
            Some(EntityDataValue::Byte(v)) => *v,
            _ => 0,
        },
    }
}

impl DisplayTrack {
    fn sample_params(&self, progress: f32) -> Params {
        if progress >= 1.0 {
            return self.to;
        }
        Params {
            transform: self.from.transform.slerp(&self.to.transform, progress),
            text: TextParams {
                opacity: lerp_int(progress, self.from.text.opacity, self.to.text.opacity),
                background: lerp_argb(progress, self.from.text.background, self.to.text.background),
            },
        }
    }

    /// One client tick of `Display.tick`.
    pub fn tick(&mut self, data: &dyn EntityData) {
        self.ticks += 1;
        let now = read_seen(data);
        let before = self.seen.as_ref();
        let delay_set = match before {
            Some(s) => s.delay != now.delay,
            None => now.delay != 0,
        };
        if delay_set {
            self.start_tick = self.ticks + i64::from(now.delay);
        }
        if before.is_none_or(|s| s.duration != now.duration) {
            self.duration = now.duration;
        }
        if before.is_none_or(|s| s.text != now.text || s.line_width != now.line_width) {
            self.text_rev += 1;
        }
        let changed = before.is_none_or(|s| {
            s.transform != now.transform
                || s.text != now.text
                || s.line_width != now.line_width
                || s.text_params != now.text_params
                || s.flags != now.flags
        });
        if changed {
            let target = Params {
                transform: now.transform,
                text: now.text_params,
            };
            self.from = if self.duration != 0 && self.live {
                self.sample_params(self.last_progress.get())
            } else {
                target
            };
            self.to = target;
            self.live = true;
        }
        self.seen = Some(now);
    }

    /// `Display.calculateInterpolationProgress`; `None` until the first tick.
    pub fn sample(&self, partial: f32) -> Option<DisplaySample> {
        if !self.live {
            return None;
        }
        let progress = if self.duration <= 0 {
            1.0
        } else {
            let since = (self.ticks - self.start_tick) as f32 + partial.clamp(0.0, 1.0);
            let p = (since / self.duration as f32).clamp(0.0, 1.0);
            self.last_progress.set(p);
            p
        };
        let params = self.sample_params(progress);
        Some(DisplaySample {
            transform: params.transform.matrix(),
            opacity: params.text.opacity as u8,
            background: params.text.background,
            text_rev: self.text_rev,
        })
    }
}

#[cfg(test)]
mod tests {
    use mcr_protocol::types::math::Vec3f;
    use rustc_hash::FxHashMap;

    use super::*;

    type Data = FxHashMap<u8, EntityDataValue>;

    fn translate(data: &mut Data, x: f32) {
        data.insert(
            idx::DISPLAY_TRANSLATION,
            EntityDataValue::Vector3(Vec3f { x, y: 0.0, z: 0.0 }),
        );
    }

    fn x_of(s: DisplaySample) -> f32 {
        s.transform.w_axis.x
    }

    #[test]
    fn no_sample_before_the_first_tick() {
        assert!(DisplayTrack::default().sample(0.0).is_none());
    }

    #[test]
    fn without_a_duration_changes_apply_at_once() {
        let mut data = Data::default();
        let mut t = DisplayTrack::default();
        t.tick(&data);
        translate(&mut data, 4.0);
        t.tick(&data);
        assert_eq!(x_of(t.sample(0.0).unwrap()), 4.0);
    }

    #[test]
    fn interpolates_from_the_last_shown_value() {
        let mut data = Data::default();
        data.insert(
            idx::DISPLAY_INTERPOLATION_DURATION,
            EntityDataValue::Int(10),
        );
        let mut t = DisplayTrack::default();
        t.tick(&data);
        translate(&mut data, 10.0);
        data.insert(idx::DISPLAY_INTERPOLATION_DELAY, EntityDataValue::Int(1));
        t.tick(&data);
        assert_eq!(x_of(t.sample(0.0).unwrap()), 0.0);
        for _ in 0..6 {
            t.tick(&data);
        }
        let mid = x_of(t.sample(0.0).unwrap());
        assert!((mid - 5.0).abs() < 0.01, "{mid}");
        for _ in 0..8 {
            t.tick(&data);
        }
        assert_eq!(x_of(t.sample(0.0).unwrap()), 10.0);
    }

    #[test]
    fn hostile_transform_is_clamped_and_finite() {
        let mut data = Data::default();
        translate(&mut data, f32::NAN);
        data.insert(
            idx::DISPLAY_SCALE,
            EntityDataValue::Vector3(Vec3f {
                x: 1.0e30,
                y: f32::INFINITY,
                z: 1.0,
            }),
        );
        data.insert(
            idx::DISPLAY_LEFT_ROTATION,
            EntityDataValue::Quaternion([f32::NAN, 0.0, 0.0, 0.0]),
        );
        let mut t = DisplayTrack::default();
        t.tick(&data);
        assert!(t.sample(0.0).unwrap().transform.is_finite());
    }

    #[test]
    fn text_colours_lerp_like_vanilla_ints() {
        assert_eq!(lerp_int(0.5, -1, 63), 31);
        assert_eq!(
            (lerp_argb(0.5, 0x4000_0000, 0x00FF_0000) >> 16) & 0xFF,
            0x7F
        );
    }

    #[test]
    fn text_revision_follows_text_and_width_only() {
        let mut data = Data::default();
        let mut t = DisplayTrack::default();
        t.tick(&data);
        let rev = t.sample(0.0).unwrap().text_rev;
        data.insert(idx::TEXT_DISPLAY_OPACITY, EntityDataValue::Byte(10));
        t.tick(&data);
        assert_eq!(t.sample(0.0).unwrap().text_rev, rev);
        data.insert(idx::TEXT_DISPLAY_LINE_WIDTH, EntityDataValue::Int(50));
        t.tick(&data);
        assert_eq!(t.sample(0.0).unwrap().text_rev, rev + 1);
    }
}
