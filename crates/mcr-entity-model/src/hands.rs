use glam::{Mat4, Vec3};
use mcr_protocol::types::ItemStack;
use mcr_protocol::types::component::DataComponent;

use crate::equipment::component;
use crate::plan::{Attach, AttachKind, ItemContext, ItemSource, Plan};
use crate::rig::ArmPose;
use crate::view::{EntityView, slot};

/// How a model family builds the pose-stack frame at its hand (vanilla `translateToHand`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HandFamily {
    Default,
    Slim,
    Skeleton,
    Allay,
    Vex,
    CopperGolem { idle: bool },
}

const SIXTEENTH: f32 = 1.0 / 16.0;

/// `ItemInHandLayer`: rotate -90 degrees about X, 180 about Y, then step to the grip point.
pub fn hand_item_matrix(left: bool, baby: bool) -> Mat4 {
    let (ox, oy, oz) = if baby {
        (0.0, 1.0, -4.5)
    } else {
        (1.0, 2.0, -10.0)
    };
    let x = if left { -ox } else { ox };
    Mat4::from_rotation_x(-90f32.to_radians())
        * Mat4::from_rotation_y(180f32.to_radians())
        * Mat4::from_translation(Vec3::new(x, oy, oz) * SIXTEENTH)
}

pub fn hand_attach(
    family: HandFamily,
    left: bool,
    baby: bool,
    source: ItemSource,
    context: ItemContext,
) -> Attach {
    let kind = AttachKind::Item { source, context };
    let arm = if left { "left_arm" } else { "right_arm" };
    let side = if left { -1.0 } else { 1.0 };
    let grip = hand_item_matrix(left, baby);
    let to_blocks = Mat4::from_scale(Vec3::splat(16.0));
    match family {
        HandFamily::Default => Attach::new(arm, kind, to_blocks * grip),
        HandFamily::Slim => {
            let mut a = Attach::new(arm, kind, to_blocks * grip);
            a.offset = Vec3::new(0.5 * side, 0.0, 0.0);
            a
        }
        HandFamily::Skeleton => {
            let mut a = Attach::new(arm, kind, to_blocks * grip);
            a.offset = Vec3::new(side, 0.0, 0.0);
            a
        }
        HandFamily::Allay => {
            let mut a = Attach::new(
                "body",
                kind,
                to_blocks * Mat4::from_translation(Vec3::new(0.0, 0.0625, 0.1875)),
            );
            a.rx_of = Some("right_arm");
            a.post = Mat4::from_scale(Vec3::splat(0.7))
                * Mat4::from_translation(Vec3::new(0.0625, 0.0, 0.0))
                * grip;
            a
        }
        HandFamily::Vex => Attach::new(
            arm,
            kind,
            to_blocks
                * Mat4::from_scale(Vec3::splat(0.55))
                * Mat4::from_translation(Vec3::new(0.046875 * side, -0.15625, 0.078125))
                * grip,
        ),
        HandFamily::CopperGolem { idle } => {
            let step = if idle {
                Mat4::from_rotation_y((90.0 * -side).to_radians())
                    * Mat4::from_translation(Vec3::new(0.0, 0.0, 0.125))
            } else {
                Mat4::from_scale(Vec3::splat(0.55))
                    * Mat4::from_translation(Vec3::new(-0.125, 0.3125, -0.1875))
            };
            Attach::new(arm, kind, to_blocks * step * grip)
        }
    }
}

pub fn item_name(stack: &ItemStack) -> Option<&'static str> {
    mcr_data::v26_3().item_name(u32::try_from(stack.item).ok()?)
}

pub fn is_item(stack: Option<&ItemStack>, name: &str) -> bool {
    stack.is_some_and(|s| item_name(s) == Some(name))
}

pub fn is_spear(stack: Option<&ItemStack>) -> bool {
    stack
        .and_then(item_name)
        .is_some_and(|n| n.ends_with("_spear"))
}

pub fn crossbow_charged(stack: &ItemStack) -> bool {
    matches!(
        component(stack, "charged_projectiles"),
        Some(DataComponent::ChargedProjectiles(list)) if !list.0.is_empty()
    )
}

/// Which arm each of main hand and off hand drives for a given handedness.
pub fn arm_items<'a>(
    v: &EntityView<'a>,
    left_handed: bool,
) -> [(Option<&'a ItemStack>, bool, bool); 2] {
    [
        (v.item(slot::MAINHAND), left_handed, true),
        (v.item(slot::OFFHAND), !left_handed, false),
    ]
}

pub fn set_pose(plan: &mut Plan, left: bool, pose: ArmPose) {
    if left {
        plan.anim.left_arm = pose;
    } else {
        plan.anim.right_arm = pose;
    }
}

/// Arm poses of a player: held items use `Item`, the used item selects its pull-back pose.
pub fn player_poses(v: &EntityView<'_>, plan: &mut Plan, left_handed: bool) {
    let using = v.living_flag(0);
    let using_off = v.living_flag(1);
    let mut main = ArmPose::Empty;
    let mut off = ArmPose::Empty;
    for (stack, _, is_main) in arm_items(v, left_handed) {
        let Some(stack) = stack else { continue };
        let name = item_name(stack);
        let used = using && (using_off != is_main);
        let mut pose = ArmPose::Item;
        if name == Some("minecraft:crossbow") && crossbow_charged(stack) && v.attack_time <= 0.0 {
            pose = ArmPose::CrossbowHold;
        } else if used {
            pose = match name {
                Some("minecraft:shield") => ArmPose::Block,
                Some("minecraft:bow") => ArmPose::BowAndArrow,
                Some("minecraft:trident") => ArmPose::ThrowTrident,
                Some("minecraft:crossbow") => ArmPose::CrossbowCharge,
                Some("minecraft:spyglass") => ArmPose::Spyglass,
                Some("minecraft:goat_horn") => ArmPose::TootHorn,
                Some("minecraft:brush") => ArmPose::Brush,
                Some(n) if n.ends_with("_spear") => ArmPose::Spear,
                _ => ArmPose::Item,
            };
        } else if is_spear(Some(stack)) {
            pose = ArmPose::Spear;
        }
        if is_main {
            main = pose;
        } else {
            off = pose;
        }
    }
    if matches!(
        main,
        ArmPose::BowAndArrow | ArmPose::CrossbowCharge | ArmPose::CrossbowHold
    ) && v.item(slot::OFFHAND).is_some()
    {
        off = ArmPose::Item;
    } else if matches!(
        main,
        ArmPose::BowAndArrow | ArmPose::CrossbowCharge | ArmPose::CrossbowHold
    ) {
        off = ArmPose::Empty;
    }
    set_pose(plan, left_handed, main);
    set_pose(plan, !left_handed, off);
    plan.anim.use_ticks = if using { v.use_ticks } else { 0.0 };
    plan.anim.charge_max = 25.0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grip_point_sits_ten_sixteenths_down_the_arm() {
        let p = hand_item_matrix(false, false).transform_point3(Vec3::ZERO) * 16.0;
        assert!((p - Vec3::new(-1.0, 10.0, -2.0)).length() < 1e-4, "{p}");
    }

    #[test]
    fn left_hand_mirrors_the_grip_x() {
        let l = hand_item_matrix(true, false).transform_point3(Vec3::ZERO);
        let r = hand_item_matrix(false, false).transform_point3(Vec3::ZERO);
        assert!((l.x + r.x).abs() < 1e-6);
    }

    #[test]
    fn skeleton_offset_moves_the_arm_pivot_not_the_item() {
        let a = hand_attach(
            HandFamily::Skeleton,
            false,
            false,
            ItemSource::Equipment(0),
            ItemContext::RightHand,
        );
        assert_eq!(a.offset, Vec3::new(1.0, 0.0, 0.0));
    }
}
