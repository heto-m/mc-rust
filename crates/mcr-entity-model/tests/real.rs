use std::path::PathBuf;

use mcr_entity_model::glam::{Mat4, Quat};
use mcr_entity_model::{
    AnimIn, AnimationSet, Binding, BoundAnimation, LayerSet, Model, Rig, animate, evaluate,
};

fn cache() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.cache/versions/26.3")
}

fn layers() -> Option<LayerSet> {
    let path = cache().join("extracted/entity_models.json");
    if !path.exists() {
        eprintln!(
            "skipping: {} missing (run tools/extractor/run-client.sh)",
            path.display()
        );
        return None;
    }
    Some(LayerSet::load(path).unwrap())
}

fn model(set: &LayerSet, name: &str) -> Model {
    Model::from_layer(set.get(name).unwrap_or_else(|| panic!("layer {name}")))
}

#[test]
fn pig_has_head_body_and_four_legs() {
    let Some(set) = layers() else { return };
    let pig = model(&set, "minecraft:pig#main");
    for part in [
        "head",
        "body",
        "right_hind_leg",
        "left_hind_leg",
        "right_front_leg",
        "left_front_leg",
    ] {
        assert!(pig.bone(part).is_some(), "{part}");
    }
    assert_eq!(pig.bones.len(), 7);
    assert_eq!(pig.cube_count(), 7);
}

#[test]
fn zombie_is_a_biped_with_hat_overlay() {
    let Some(set) = layers() else { return };
    let zombie = model(&set, "minecraft:zombie#main");
    assert!(zombie.bone("hat").is_some());
    assert_eq!(zombie.cube_count(), 7);
}

#[test]
fn every_layer_builds_with_valid_vertices() {
    let Some(set) = layers() else { return };
    assert!(set.0.len() > 300);
    for (name, def) in &set.0 {
        let m = Model::from_layer(def);
        assert_eq!(m.vertices.len() % 4, 0, "{name}");
        assert!(m.vertices.iter().all(|v| (v.bone as usize) < m.bones.len()));
        assert!(
            m.vertices
                .iter()
                .all(|v| v.pos.iter().all(|c| c.is_finite()))
        );
    }
}

#[test]
fn bat_resting_animation_samples_its_keyframe() {
    let path = cache().join("extracted/animations.json");
    if !path.exists() {
        return;
    }
    let anims = AnimationSet::load(path).unwrap();
    assert!(anims.0.len() > 50);
    let Some(set) = layers() else { return };
    let bat = model(&set, "minecraft:bat#main");
    let anim = BoundAnimation::bind(anims.get("BatAnimation.BAT_RESTING").unwrap(), &bat);
    let mut pose = bat.rest_pose();
    anim.apply(0.0, 1.0, &mut pose);
    let head = bat.bone("head").unwrap() as usize;
    assert!((pose[head].rot.x - std::f32::consts::PI).abs() < 1e-5);
}

#[test]
fn chicken_head_beak_and_wattle_turn_together() {
    let Some(set) = layers() else { return };
    for layer in ["minecraft:chicken#main", "minecraft:cold_chicken#main"] {
        let chicken = model(&set, layer);
        let head = chicken.bone("head").unwrap();
        for part in ["beak", "red_thing"] {
            let bone = chicken.bone(part).unwrap();
            assert_eq!(chicken.bones[bone as usize].parent, Some(head), "{part}");
        }
        let mut pose = chicken.rest_pose();
        let input = AnimIn {
            head_yaw: 50.0,
            head_pitch: 30.0,
            on_ground: true,
            ..AnimIn::default()
        };
        animate(Rig::Chicken, &input, &Binding::new(&chicken), &mut pose);
        let mut world = vec![Mat4::IDENTITY; chicken.bones.len()];
        evaluate(&chicken, &pose, &mut world);
        let rotation = |m: Mat4| Quat::from_mat4(&m).normalize();
        let head_rot = rotation(world[head as usize]);
        for part in ["beak", "red_thing"] {
            let bone = chicken.bone(part).unwrap();
            assert!(rotation(world[bone as usize]).angle_between(head_rot) < 1.0e-4);
        }
        assert!(head_rot.angle_between(Quat::IDENTITY) > 0.5);
    }
}
