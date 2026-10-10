use mcr_protocol::types::entity_data::{ArmadilloState, EntityDataValue, Pose, SnifferState};

use crate::catalog::PlanCx;
use crate::idx;
use crate::plan::{Plan, Pose as PartPose};
use crate::rig::Rig;
use crate::view::EntityView;

/// Seconds into a looping or repeating animation of `length` seconds driven by the entity's age.
fn cycle(v: &EntityView<'_>, length: f32, speed: f32) -> f32 {
    if length <= 0.0 {
        return 0.0;
    }
    (v.age_ticks * 0.05 * speed) % length
}

/// vanilla `animateWalk`: limb swing scaled to milliseconds, weight from the swing amount.
fn walk(plan: &mut Plan, v: &EntityView<'_>, name: &'static str, speed: f32, scale: f32) {
    let weight = (v.limb_amount * scale).min(1.0);
    if weight > 0.0 {
        plan.keyframes
            .push((name, v.limb_swing * 0.05 * speed, weight));
    }
}

/// Drives the extracted keyframe animations of the types that have them from pose and state.
pub fn drive(cx: &PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
    if plan.parts.is_empty() {
        return;
    }
    let Some(name) = cx.type_name(v) else { return };
    let pose = v.pose();
    let baby = plan.anim.baby;
    match name {
        "minecraft:armadillo" => {
            let state = match v.raw(idx::ARMADILLO_STATE) {
                Some(EntityDataValue::ArmadilloState(s)) => *s,
                _ => ArmadilloState::Idle,
            };
            let (walk_name, roll) = if baby {
                (
                    "BabyArmadilloAnimation.ARMADILLO_BABY_WALK",
                    "BabyArmadilloAnimation.ARMADILLO_BABY_ROLL_UP",
                )
            } else {
                (
                    "ArmadilloAnimation.ARMADILLO_WALK",
                    "ArmadilloAnimation.ARMADILLO_ROLL_UP",
                )
            };
            match state {
                ArmadilloState::Rolling | ArmadilloState::Scared => {
                    plan.keyframes.push((roll, 10.0, 1.0));
                }
                _ => walk(plan, v, walk_name, 16.0, 2.5),
            }
        }
        "minecraft:camel" | "minecraft:camel_husk" => {
            if pose == Pose::Sitting {
                plan.keyframes
                    .push(("CamelAnimation.CAMEL_SIT_POSE", 10.0, 1.0));
            } else {
                walk(plan, v, "CamelAnimation.CAMEL_WALK", 2.0, 2.5);
                plan.keyframes
                    .push(("CamelAnimation.CAMEL_IDLE", cycle(v, 4.0, 1.0), 1.0));
            }
        }
        "minecraft:sniffer" => {
            let state = match v.raw(idx::SNIFFER_STATE) {
                Some(EntityDataValue::SnifferState(s)) => *s,
                _ => SnifferState::Idling,
            };
            match state {
                SnifferState::FeelingHappy => {
                    plan.keyframes
                        .push(("SnifferAnimation.SNIFFER_HAPPY", cycle(v, 2.0, 1.0), 1.0))
                }
                SnifferState::Searching | SnifferState::Scenting => plan.keyframes.push((
                    "SnifferAnimation.SNIFFER_SNIFF_SEARCH",
                    cycle(v, 2.0, 1.0),
                    1.0,
                )),
                SnifferState::Sniffing => plan.keyframes.push((
                    "SnifferAnimation.SNIFFER_LONGSNIFF",
                    cycle(v, 1.0, 1.0),
                    1.0,
                )),
                SnifferState::Digging => {
                    plan.keyframes
                        .push(("SnifferAnimation.SNIFFER_DIG", cycle(v, 8.0, 1.0), 1.0))
                }
                _ => walk(plan, v, "SnifferAnimation.SNIFFER_WALK", 9.0, 100.0),
            }
        }
        "minecraft:frog" => match pose {
            Pose::Croaking => {
                plan.keyframes
                    .push(("FrogAnimation.FROG_CROAK", cycle(v, 3.0, 1.0), 1.0))
            }
            Pose::Swimming => walk(plan, v, "FrogAnimation.FROG_SWIM", 2.0, 2.5),
            _ => walk(plan, v, "FrogAnimation.FROG_WALK", 1.5, 2.5),
        },
        "minecraft:breeze" => {
            let (n, len) = match pose {
                Pose::Shooting => ("BreezeAnimation.SHOOT", 1.125),
                Pose::Inhaling => ("BreezeAnimation.INHALE", 2.0),
                Pose::Sliding => ("BreezeAnimation.SLIDE", 0.2),
                Pose::LongJumping => ("BreezeAnimation.JUMP", 0.5),
                _ => ("BreezeAnimation.IDLE", 2.0),
            };
            plan.keyframes.push((n, cycle(v, len, 1.0), 1.0));
        }
        "minecraft:warden" => {
            let (n, len) = match pose {
                Pose::Roaring => ("WardenAnimation.WARDEN_ROAR", 4.2),
                Pose::Sniffing => ("WardenAnimation.WARDEN_SNIFF", 4.16),
                Pose::Emerging => ("WardenAnimation.WARDEN_EMERGE", 6.68),
                Pose::Digging => ("WardenAnimation.WARDEN_DIG", 5.0),
                _ => ("", 0.0),
            };
            if len > 0.0 {
                plan.keyframes.push((n, cycle(v, len, 1.0), 1.0));
            }
        }
        "minecraft:bat" => {
            let resting = v.byte(idx::BAT_FLAGS).unwrap_or(0) & 1 != 0;
            let n = if resting {
                "BatAnimation.BAT_RESTING"
            } else {
                "BatAnimation.BAT_FLYING"
            };
            plan.keyframes.push((n, cycle(v, 0.5, 1.0), 1.0));
        }
        "minecraft:rabbit" => {
            if v.limb_amount > 0.01 {
                let n = if baby {
                    "BabyRabbitAnimation.HOP"
                } else {
                    "RabbitAnimation.HOP"
                };
                plan.keyframes
                    .push((n, cycle(v, 0.75, 1.5), v.limb_amount.min(1.0)));
            }
        }
        "minecraft:copper_golem" => {
            walk(plan, v, "CopperGolemAnimation.COPPER_GOLEM_WALK", 4.0, 2.5);
            plan.keyframes.push((
                "CopperGolemAnimation.COPPER_GOLEM_IDLE",
                cycle(v, 3.5, 1.0),
                1.0,
            ));
        }
        "minecraft:creaking" => walk(plan, v, "CreakingAnimation.CREAKING_WALK", 2.0, 2.5),
        _ => return,
    }
    plan.rig = Rig::Static;
    for part in &mut plan.parts {
        if part.pose == PartPose::Rig {
            part.pose = PartPose::Keyframes;
        }
    }
}
