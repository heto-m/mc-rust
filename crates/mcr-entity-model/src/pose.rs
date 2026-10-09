use glam::{EulerRot, Mat4, Quat, Vec3};

use crate::model::Model;

/// Local bone transform in model space (sixteenths of a block); rotation is ZYX Euler like vanilla.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pose {
    pub pos: Vec3,
    pub rot: Vec3,
    pub scale: Vec3,
}

impl Pose {
    pub const IDENTITY: Pose = Pose {
        pos: Vec3::ZERO,
        rot: Vec3::ZERO,
        scale: Vec3::ONE,
    };

    pub fn matrix(&self) -> Mat4 {
        let rotation = Quat::from_euler(EulerRot::XYZ, self.rot.x, self.rot.y, self.rot.z);
        Mat4::from_scale_rotation_translation(self.scale, rotation, self.pos)
    }
}

/// World matrices (pose chain only, still in sixteenths) for every bone of `model`.
pub fn evaluate(model: &Model, pose: &[Pose], out: &mut [Mat4]) {
    for (i, bone) in model.bones.iter().enumerate() {
        let local = pose[i].matrix();
        out[i] = match bone.parent {
            Some(p) => out[p as usize] * local,
            None => local,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::LayerSet;

    #[test]
    fn child_follows_parent_rotation() {
        let json = r#"{"t":{"tex_w":16,"tex_h":16,"root":{"pose":[0,0,0,0,0,0,1,1,1],"cubes":[],"children":{
            "a":{"pose":[0,10,0,0,0,1.5707964,1,1,1],"cubes":[],"children":{
                "b":{"pose":[4,0,0,0,0,0,1,1,1],"cubes":[],"children":{}}}}}}}}"#;
        let set = LayerSet::from_json(json).unwrap();
        let model = Model::from_layer(set.get("t").unwrap());
        let mut world = vec![Mat4::IDENTITY; model.bones.len()];
        evaluate(&model, &model.rest_pose(), &mut world);
        let b = world[model.bone("b").unwrap() as usize].transform_point3(Vec3::ZERO);
        assert!((b - Vec3::new(0.0, 14.0, 0.0)).length() < 1e-4, "{b}");
    }
}
