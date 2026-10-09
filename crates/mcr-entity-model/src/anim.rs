use glam::Vec3;

use crate::data::{AnimationDef, Interp, KeyframeDef, Target};
use crate::model::{BoneId, Model};
use crate::pose::Pose;

#[derive(Debug, Clone)]
struct Entry {
    bone: BoneId,
    target: Target,
    keyframes: Vec<KeyframeDef>,
}

/// An animation resolved against one model's bones; channels for missing bones are dropped.
#[derive(Debug, Clone)]
pub struct BoundAnimation {
    pub length: f32,
    pub looping: bool,
    entries: Vec<Entry>,
}

impl BoundAnimation {
    pub fn bind(def: &AnimationDef, model: &Model) -> Self {
        let mut entries = Vec::new();
        for (name, channels) in &def.bones {
            let Some(bone) = model.bone(name) else {
                continue;
            };
            for ch in channels {
                if !ch.keyframes.is_empty() {
                    entries.push(Entry {
                        bone,
                        target: ch.target,
                        keyframes: ch.keyframes.clone(),
                    });
                }
            }
        }
        BoundAnimation {
            length: def.length,
            looping: def.looping,
            entries,
        }
    }

    /// Adds the sampled channel values onto `pose`; `scale` weights them like vanilla's walk blend.
    pub fn apply(&self, seconds: f32, scale: f32, pose: &mut [Pose]) {
        let t = if self.looping && self.length > 0.0 {
            seconds % self.length
        } else {
            seconds
        };
        for e in &self.entries {
            let v = sample(&e.keyframes, t, scale);
            let p = &mut pose[e.bone as usize];
            match e.target {
                Target::Position => p.pos += v,
                Target::Rotation => p.rot += v,
                Target::Scale => p.scale += v,
            }
        }
    }
}

pub fn catmull_rom(alpha: f32, p0: f32, p1: f32, p2: f32, p3: f32) -> f32 {
    0.5 * (2.0 * p1
        + (p2 - p0) * alpha
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * alpha * alpha
        + (3.0 * p1 - p0 - 3.0 * p2 + p3) * alpha * alpha * alpha)
}

pub fn sample(keys: &[KeyframeDef], t: f32, target_scale: f32) -> Vec3 {
    let first_at_or_after = keys.partition_point(|k| t > k.t);
    let prev = first_at_or_after.saturating_sub(1);
    let next = (prev + 1).min(keys.len() - 1);
    let (a, b) = (&keys[prev], &keys[next]);
    let alpha = if next != prev {
        ((t - a.t) / (b.t - a.t)).clamp(0.0, 1.0)
    } else {
        0.0
    };
    match b.interp {
        Interp::Linear => Vec3::from(a.post).lerp(Vec3::from(b.pre), alpha) * target_scale,
        Interp::Catmullrom => {
            let p0 = Vec3::from(keys[prev.saturating_sub(1)].post);
            let p1 = Vec3::from(a.post);
            let p2 = Vec3::from(b.post);
            let p3 = Vec3::from(keys[(next + 1).min(keys.len() - 1)].post);
            Vec3::new(
                catmull_rom(alpha, p0.x, p1.x, p2.x, p3.x),
                catmull_rom(alpha, p0.y, p1.y, p2.y, p3.y),
                catmull_rom(alpha, p0.z, p1.z, p2.z, p3.z),
            ) * target_scale
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kf(t: f32, v: [f32; 3], interp: Interp) -> KeyframeDef {
        KeyframeDef {
            t,
            pre: v,
            post: v,
            interp,
        }
    }

    fn close(a: Vec3, b: [f32; 3]) -> bool {
        (a - Vec3::from(b)).length() < 1e-5
    }

    #[test]
    fn linear_hits_keyframes_and_midpoints() {
        let keys = [
            kf(0.0, [0.0, 0.0, 0.0], Interp::Linear),
            kf(1.0, [2.0, 4.0, 6.0], Interp::Linear),
            kf(2.0, [0.0, 0.0, 0.0], Interp::Linear),
        ];
        assert!(close(sample(&keys, 0.0, 1.0), [0.0; 3]));
        assert!(close(sample(&keys, 1.0, 1.0), [2.0, 4.0, 6.0]));
        assert!(close(sample(&keys, 2.0, 1.0), [0.0; 3]));
        assert!(close(sample(&keys, 0.5, 1.0), [1.0, 2.0, 3.0]));
        assert!(close(sample(&keys, 5.0, 1.0), [0.0; 3]));
        assert!(close(sample(&keys, 0.5, 0.5), [0.5, 1.0, 1.5]));
    }

    #[test]
    fn catmull_rom_passes_through_keyframes() {
        let keys = [
            kf(0.0, [0.0; 3], Interp::Catmullrom),
            kf(1.0, [1.0, 0.0, 0.0], Interp::Catmullrom),
            kf(2.0, [0.0, 3.0, 0.0], Interp::Catmullrom),
            kf(3.0, [0.0, 0.0, 5.0], Interp::Catmullrom),
        ];
        for k in &keys {
            assert!(close(sample(&keys, k.t, 1.0), k.post), "t={}", k.t);
        }
    }

    #[test]
    fn looping_wraps_time() {
        let def = AnimationDef {
            length: 2.0,
            looping: true,
            bones: [(
                "root".to_owned(),
                vec![crate::data::ChannelDef {
                    target: Target::Rotation,
                    keyframes: vec![
                        kf(0.0, [0.0; 3], Interp::Linear),
                        kf(2.0, [2.0, 0.0, 0.0], Interp::Linear),
                    ],
                }],
            )]
            .into(),
        };
        let layer = crate::data::LayerSet::from_json(
            r#"{"t":{"tex_w":16,"tex_h":16,"root":{"pose":[0,0,0,0,0,0,1,1,1],"cubes":[],"children":{}}}}"#,
        )
        .unwrap();
        let model = Model::from_layer(layer.get("t").unwrap());
        let anim = BoundAnimation::bind(&def, &model);
        let mut pose = model.rest_pose();
        anim.apply(3.0, 1.0, &mut pose);
        assert!((pose[0].rot.x - 1.0).abs() < 1e-5);
    }
}
