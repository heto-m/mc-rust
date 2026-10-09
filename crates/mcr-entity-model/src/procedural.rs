use std::f32::consts::PI;

use crate::model::{BoneId, Model};
use crate::pose::Pose;

/// Per-entity inputs of vanilla's `setupAnim` that the hand-written rigs consume.
#[derive(Debug, Clone, Copy, Default)]
pub struct AnimState {
    pub walk_pos: f32,
    pub walk_speed: f32,
    pub head_yaw_deg: f32,
    pub head_pitch_deg: f32,
    pub age_ticks: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct Biped {
    pub head: BoneId,
    pub right_arm: BoneId,
    pub left_arm: BoneId,
    pub right_leg: BoneId,
    pub left_leg: BoneId,
}

#[derive(Debug, Clone, Copy)]
pub struct Quadruped {
    pub head: BoneId,
    pub right_hind: BoneId,
    pub left_hind: BoneId,
    pub right_front: BoneId,
    pub left_front: BoneId,
}

#[derive(Debug, Clone, Copy)]
pub enum Rig {
    Biped(Biped),
    Quadruped(Quadruped),
    HeadOnly(BoneId),
    Static,
}

impl Rig {
    pub fn detect(model: &Model) -> Rig {
        let b = |n: &str| model.bone(n);
        if let (Some(head), Some(ra), Some(la), Some(rl), Some(ll)) = (
            b("head"),
            b("right_arm"),
            b("left_arm"),
            b("right_leg"),
            b("left_leg"),
        ) {
            return Rig::Biped(Biped {
                head,
                right_arm: ra,
                left_arm: la,
                right_leg: rl,
                left_leg: ll,
            });
        }
        if let (Some(head), Some(rh), Some(lh), Some(rf), Some(lf)) = (
            b("head"),
            b("right_hind_leg"),
            b("left_hind_leg"),
            b("right_front_leg"),
            b("left_front_leg"),
        ) {
            return Rig::Quadruped(Quadruped {
                head,
                right_hind: rh,
                left_hind: lh,
                right_front: rf,
                left_front: lf,
            });
        }
        match b("head") {
            Some(head) => Rig::HeadOnly(head),
            None => Rig::Static,
        }
    }

    /// Overwrites the rotations vanilla's `setupAnim` assigns; every other bone keeps its pose.
    pub fn apply(&self, s: &AnimState, pose: &mut [Pose]) {
        let deg = PI / 180.0;
        let look = |pose: &mut [Pose], head: BoneId| {
            pose[head as usize].rot.x = s.head_pitch_deg * deg;
            pose[head as usize].rot.y = s.head_yaw_deg * deg;
        };
        let phase = s.walk_pos * 0.6662;
        match *self {
            Rig::Static => {}
            Rig::HeadOnly(head) => look(pose, head),
            Rig::Quadruped(q) => {
                look(pose, q.head);
                let swing = |offset: f32| (phase + offset).cos() * 1.4 * s.walk_speed;
                pose[q.right_hind as usize].rot.x = swing(0.0);
                pose[q.left_hind as usize].rot.x = swing(PI);
                pose[q.right_front as usize].rot.x = swing(PI);
                pose[q.left_front as usize].rot.x = swing(0.0);
            }
            Rig::Biped(b) => {
                look(pose, b.head);
                let arm = |offset: f32| (phase + offset).cos() * s.walk_speed;
                let leg = |offset: f32| (phase + offset).cos() * 1.4 * s.walk_speed;
                pose[b.right_arm as usize].rot.x = arm(PI);
                pose[b.left_arm as usize].rot.x = arm(0.0);
                pose[b.right_leg as usize].rot.x = leg(0.0);
                pose[b.left_leg as usize].rot.x = leg(PI);
                pose[b.right_leg as usize].rot.y = 0.005;
                pose[b.left_leg as usize].rot.y = -0.005;
                pose[b.right_leg as usize].rot.z = 0.005;
                pose[b.left_leg as usize].rot.z = -0.005;
                for (bone, dir) in [(b.right_arm, 1.0), (b.left_arm, -1.0)] {
                    let p = &mut pose[bone as usize];
                    p.rot.z += dir * ((s.age_ticks * 0.09).cos() * 0.05 + 0.05);
                    p.rot.x += dir * (s.age_ticks * 0.067).sin() * 0.05;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::LayerSet;

    fn quadruped() -> Model {
        let leg = r#"{"pose":[0,0,0,0,0,0,1,1,1],"cubes":[],"children":{}}"#;
        let json = format!(
            r#"{{"t":{{"tex_w":16,"tex_h":16,"root":{{"pose":[0,0,0,0,0,0,1,1,1],"cubes":[],"children":{{
            "head":{leg},"body":{leg},"right_hind_leg":{leg},"left_hind_leg":{leg},
            "right_front_leg":{leg},"left_front_leg":{leg}}}}}}}}}"#
        );
        Model::from_layer(LayerSet::from_json(&json).unwrap().get("t").unwrap())
    }

    #[test]
    fn standing_still_keeps_limbs_at_rest() {
        let model = quadruped();
        let rig = Rig::detect(&model);
        assert!(matches!(rig, Rig::Quadruped(_)));
        let mut pose = model.rest_pose();
        rig.apply(&AnimState::default(), &mut pose);
        assert!(pose.iter().all(|p| p.rot.x == 0.0));
    }

    #[test]
    fn walking_swings_diagonal_legs_together() {
        let model = quadruped();
        let rig = Rig::detect(&model);
        let mut pose = model.rest_pose();
        let state = AnimState {
            walk_speed: 1.0,
            head_yaw_deg: 90.0,
            ..AnimState::default()
        };
        rig.apply(&state, &mut pose);
        let rot = |n: &str| pose[model.bone(n).unwrap() as usize].rot;
        assert!((rot("right_hind_leg").x - 1.4).abs() < 1e-5);
        assert!((rot("left_front_leg").x - 1.4).abs() < 1e-5);
        assert!((rot("left_hind_leg").x + 1.4).abs() < 1e-5);
        assert!((rot("head").y - PI / 2.0).abs() < 1e-5);
    }
}
