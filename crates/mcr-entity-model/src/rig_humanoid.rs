use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};

use glam::Vec3;

use crate::pose::Pose;
use crate::rig::{
    AnimIn, ArmPose, DEG, HumanoidStyle, IllagerPose, PiglinPose, R, Slot, cos, lerp,
};

fn sin(x: f32) -> f32 {
    x.sin()
}

fn progress(t: f32, start: f32, end: f32) -> f32 {
    ((t - start) / (end - start)).clamp(0.0, 1.0)
}

pub(crate) fn humanoid(r: &mut R, a: &AnimIn, style: HumanoidStyle) {
    let head_x = a.head_pitch * DEG;
    let head_y = a.head_yaw * DEG;
    r.look(a);
    if a.fall_flying {
        r.rx(Slot::Head, -FRAC_PI_4);
    }
    let (pos, sp) = (a.walk_pos, a.walk_speed / a.speed_value.max(1.0));
    r.rx(Slot::RightArm, cos(pos * 0.6662 + PI) * 2.0 * sp * 0.5);
    r.rx(Slot::LeftArm, cos(pos * 0.6662) * 2.0 * sp * 0.5);
    r.rot(
        Slot::RightLeg,
        Some(cos(pos * 0.6662) * 1.4 * sp),
        Some(0.005),
        Some(0.005),
    );
    r.rot(
        Slot::LeftLeg,
        Some(cos(pos * 0.6662 + PI) * 1.4 * sp),
        Some(-0.005),
        Some(-0.005),
    );
    if a.passenger {
        sit_on_mount(r);
    }

    let prioritize_right = if a.use_ticks > 0.0 {
        a.main_right
    } else {
        let off_is_right = !a.main_right;
        let off_pose = if off_is_right {
            a.right_arm
        } else {
            a.left_arm
        };
        if off_pose.two_handed() {
            a.main_right
        } else {
            off_is_right
        }
    };
    if prioritize_right {
        pose_arm(r, a, true, head_x, head_y);
        if !a.right_arm.affects_offhand() {
            pose_arm(r, a, false, head_x, head_y);
        }
    } else {
        pose_arm(r, a, false, head_x, head_y);
        if !a.left_arm.affects_offhand() {
            pose_arm(r, a, true, head_x, head_y);
        }
    }

    if style == HumanoidStyle::Piglin && a.piglin == PiglinPose::MeleeAttack && a.attack > 0.0 {
        swing_weapon_down(r, a);
    } else {
        attack(r, a, head_x);
    }
    if matches!(
        style,
        HumanoidStyle::Zombie | HumanoidStyle::ZombifiedPiglin | HumanoidStyle::Drowned
    ) {
        zombie_arms(r, a);
    }
    if a.crouching {
        r.rx(Slot::Body, 0.5);
        r.add_rot(Slot::RightArm, Vec3::new(0.4, 0.0, 0.0));
        r.add_rot(Slot::LeftArm, Vec3::new(0.4, 0.0, 0.0));
        r.add_pos(Slot::RightLeg, Vec3::new(0.0, 0.0, 4.0));
        r.add_pos(Slot::LeftLeg, Vec3::new(0.0, 0.0, 4.0));
        r.add_pos(Slot::Head, Vec3::new(0.0, 4.2, 0.0));
        r.add_pos(Slot::Body, Vec3::new(0.0, 3.2, 0.0));
        r.add_pos(Slot::LeftArm, Vec3::new(0.0, 3.2, 0.0));
        r.add_pos(Slot::RightArm, Vec3::new(0.0, 3.2, 0.0));
    }
    if a.right_arm != ArmPose::Spyglass {
        bob(r, Slot::RightArm, a.age, 1.0);
    }
    if a.left_arm != ArmPose::Spyglass {
        bob(r, Slot::LeftArm, a.age, -1.0);
    }
    if a.swim_amount > 0.0 {
        if style == HumanoidStyle::Drowned {
            drowned_swim(r, a);
        } else {
            swim(r, a);
        }
    }
    match style {
        HumanoidStyle::Skeleton => skeleton(r, a),
        HumanoidStyle::Piglin | HumanoidStyle::ZombifiedPiglin => piglin(r, a, style),
        _ => {}
    }
}

fn sit_on_mount(r: &mut R) {
    r.add_rot(Slot::RightArm, Vec3::new(-PI / 5.0, 0.0, 0.0));
    r.add_rot(Slot::LeftArm, Vec3::new(-PI / 5.0, 0.0, 0.0));
    r.rot(
        Slot::RightLeg,
        Some(-1.4137167),
        Some(PI / 10.0),
        Some(0.07853982),
    );
    r.rot(
        Slot::LeftLeg,
        Some(-1.4137167),
        Some(-PI / 10.0),
        Some(-0.07853982),
    );
}

fn skeleton(r: &mut R, a: &AnimIn) {
    if a.aggressive && !a.holding_bow {
        let s2 = sin(a.attack * PI);
        let s1 = sin((1.0 - (1.0 - a.attack) * (1.0 - a.attack)) * PI);
        r.rz(Slot::RightArm, 0.0);
        r.rz(Slot::LeftArm, 0.0);
        r.ry(Slot::RightArm, -(0.1 - s2 * 0.6));
        r.ry(Slot::LeftArm, 0.1 - s2 * 0.6);
        r.rx(Slot::RightArm, -FRAC_PI_2 - (s2 * 1.2 - s1 * 0.4));
        r.rx(Slot::LeftArm, -FRAC_PI_2 - (s2 * 1.2 - s1 * 0.4));
        bob(r, Slot::RightArm, a.age, 1.0);
        bob(r, Slot::LeftArm, a.age, -1.0);
    }
}

fn piglin(r: &mut R, a: &AnimIn, style: HumanoidStyle) {
    let baby_ear = if a.baby { 5.0 } else { 30.0 } * DEG;
    let freq = a.age * 0.1 + a.walk_pos * 0.5;
    let amp = 0.08 + a.walk_speed * 0.4;
    r.rz(Slot::LeftEar, -baby_ear - cos(freq * 1.2) * amp);
    r.rz(Slot::RightEar, baby_ear + cos(freq) * amp);
    if style != HumanoidStyle::Piglin {
        return;
    }
    let head_x = a.head_pitch * DEG;
    let head_y = a.head_yaw * DEG;
    match a.piglin {
        PiglinPose::Dancing => {
            let t = a.age / 60.0;
            r.rz(Slot::RightEar, PI / 6.0 + DEG * sin(t * 30.0) * 10.0);
            r.rz(Slot::LeftEar, -PI / 6.0 - DEG * cos(t * 30.0) * 10.0);
            r.add_pos(
                Slot::Head,
                Vec3::new(sin(t * 10.0), sin(t * 40.0) + 0.4, 0.0),
            );
            let z = DEG * (70.0 + cos(t * 40.0) * 10.0);
            r.rz(Slot::RightArm, z);
            r.rz(Slot::LeftArm, -z);
            r.add_pos(
                Slot::RightArm,
                Vec3::new(0.0, sin(t * 40.0) * 0.5 - 0.5, 0.0),
            );
            r.add_pos(
                Slot::LeftArm,
                Vec3::new(0.0, sin(t * 40.0) * 0.5 + 0.5, 0.0),
            );
            r.add_pos(Slot::Body, Vec3::new(0.0, sin(t * 40.0) * 0.35, 0.0));
        }
        PiglinPose::MeleeAttack if a.attack == 0.0 => {
            r.rx(
                if a.main_right {
                    Slot::RightArm
                } else {
                    Slot::LeftArm
                },
                -1.8,
            );
        }
        PiglinPose::CrossbowHold => crossbow_hold(r, head_x, head_y, a.main_right),
        PiglinPose::CrossbowCharge => crossbow_charge(r, a, a.main_right),
        PiglinPose::Admiring => {
            r.rx(Slot::Head, 0.5);
            r.ry(Slot::Head, 0.0);
            if a.main_right {
                r.ry(Slot::LeftArm, 0.5);
                r.rx(Slot::LeftArm, -0.9);
            } else {
                r.ry(Slot::RightArm, -0.5);
                r.rx(Slot::RightArm, -0.9);
            }
        }
        _ => {}
    }
}

fn bob(r: &mut R, s: Slot, age: f32, scale: f32) {
    r.add_rot(
        s,
        Vec3::new(
            scale * (age * 0.067).sin() * 0.05,
            0.0,
            scale * ((age * 0.09).cos() * 0.05 + 0.05),
        ),
    );
}

pub(crate) fn bob_arms(r: &mut R, age: f32) {
    bob(r, Slot::RightArm, age, 1.0);
    bob(r, Slot::LeftArm, age, -1.0);
}

fn zombie_arms(r: &mut R, a: &AnimIn) {
    if a.right_arm == ArmPose::Spear || a.left_arm == ArmPose::Spear {
        return;
    }
    let raise = !a.baby || a.main_hand_empty;
    let drop = if raise {
        -PI / if a.aggressive { 1.5 } else { 2.25 }
    } else {
        0.0
    };
    let ay = (if raise { 1.0 } else { -1.0 }) * sin(a.attack * PI);
    let ax = sin((1.0 - (1.0 - a.attack) * (1.0 - a.attack)) * PI);
    let x = drop + ay * 1.2 - ax * 0.4;
    let y = 0.1 - ay * 0.6;
    if a.right_arm != ArmPose::ThrowTrident {
        r.rot(
            Slot::RightArm,
            Some(x),
            Some(if raise { -y } else { y }),
            Some(0.0),
        );
    }
    if a.left_arm != ArmPose::ThrowTrident {
        r.rot(
            Slot::LeftArm,
            Some(x),
            Some(if raise { y } else { -y }),
            Some(0.0),
        );
    }
}

fn crossbow_hold(r: &mut R, head_x: f32, head_y: f32, holding_right: bool) {
    let (hold, shoot) = if holding_right {
        (Slot::RightArm, Slot::LeftArm)
    } else {
        (Slot::LeftArm, Slot::RightArm)
    };
    r.ry(hold, if holding_right { -0.3 } else { 0.3 } + head_y);
    r.ry(shoot, if holding_right { 0.6 } else { -0.6 } + head_y);
    r.rx(hold, -FRAC_PI_2 + head_x + 0.1);
    r.rx(shoot, -1.5 + head_x);
}

fn crossbow_charge(r: &mut R, a: &AnimIn, holding_right: bool) {
    let (hold, pull) = if holding_right {
        (Slot::RightArm, Slot::LeftArm)
    } else {
        (Slot::LeftArm, Slot::RightArm)
    };
    r.ry(hold, if holding_right { -0.8 } else { 0.8 });
    r.rx(hold, -0.97079635);
    let alpha = if a.charge_max > 0.0 {
        a.use_ticks.clamp(0.0, a.charge_max) / a.charge_max
    } else {
        0.0
    };
    r.ry(
        pull,
        lerp(alpha, 0.4, 0.85) * if holding_right { 1.0 } else { -1.0 },
    );
    r.rx(pull, lerp(alpha, -0.97079635, -FRAC_PI_2));
}

fn pose_arm(r: &mut R, a: &AnimIn, right: bool, head_x: f32, head_y: f32) {
    let arm = if right { Slot::RightArm } else { Slot::LeftArm };
    let pose = if right { a.right_arm } else { a.left_arm };
    let arm_x = r.rotation(arm).x;
    match pose {
        ArmPose::Empty => r.ry(arm, 0.0),
        ArmPose::Item => {
            r.rot(arm, Some(arm_x * 0.5 - PI / 10.0), Some(0.0), None);
        }
        ArmPose::Block => {
            let side = if right { -30.0 } else { 30.0 } * DEG;
            r.rot(
                arm,
                Some(arm_x * 0.5 - 0.9424779 + head_x.clamp(-PI * 4.0 / 9.0, 0.43633232)),
                Some(side + head_y.clamp(-PI / 6.0, PI / 6.0)),
                None,
            );
        }
        ArmPose::BowAndArrow => {
            let (ry, ly) = if right {
                (-0.1 + head_y, 0.1 + head_y + 0.4)
            } else {
                (-0.1 + head_y - 0.4, 0.1 + head_y)
            };
            r.ry(Slot::RightArm, ry);
            r.ry(Slot::LeftArm, ly);
            r.rx(Slot::RightArm, -FRAC_PI_2 + head_x);
            r.rx(Slot::LeftArm, -FRAC_PI_2 + head_x);
        }
        ArmPose::ThrowTrident => {
            r.rot(arm, Some(arm_x * 0.5 - PI), Some(0.0), None);
        }
        ArmPose::CrossbowCharge => crossbow_charge(r, a, right),
        ArmPose::CrossbowHold => crossbow_hold(r, head_x, head_y, right),
        ArmPose::Spyglass => {
            let crouch = if a.crouching { PI / 12.0 } else { 0.0 };
            let side = if right { -PI / 12.0 } else { PI / 12.0 };
            r.rot(
                arm,
                Some((head_x - 1.9198622 - crouch).clamp(-2.4, 3.3)),
                Some(head_y + side),
                None,
            );
        }
        ArmPose::TootHorn => {
            let side = if right { -PI / 6.0 } else { PI / 6.0 };
            r.rot(
                arm,
                Some(head_x.clamp(-1.2, 1.2) - 1.4835298),
                Some(head_y + side),
                None,
            );
        }
        ArmPose::Brush => {
            r.rot(arm, Some(arm_x * 0.5 - PI / 5.0), Some(0.0), None);
        }
        ArmPose::Spear => {
            let invert = if right { 1.0 } else { -1.0 };
            let mut y = -0.1 * invert + head_y;
            let mut x = -FRAC_PI_2 + head_x + 0.8;
            if a.fall_flying || a.swim_amount > 0.0 {
                x -= 0.9599311;
            }
            y = (y / DEG).clamp(-60.0, 60.0) * DEG;
            x = (x / DEG).clamp(-120.0, 30.0) * DEG;
            r.rot(arm, Some(x), Some(y), None);
        }
    }
}

fn attack(r: &mut R, a: &AnimIn, head_x: f32) {
    let t = a.attack;
    if t <= 0.0 {
        return;
    }
    let mut body_y = (t.sqrt() * PI * 2.0).sin() * 0.2;
    if a.swing_left {
        body_y = -body_y;
    }
    r.ry(Slot::Body, body_y);
    let age_scale = if a.baby { 0.5 } else { 1.0 };
    if let Some(p) = r.p(Slot::RightArm) {
        p.pos.z = body_y.sin() * 5.0 * age_scale;
        p.pos.x = -body_y.cos() * 5.0 * age_scale;
        p.rot.y += body_y;
    }
    if let Some(p) = r.p(Slot::LeftArm) {
        p.pos.z = -body_y.sin() * 5.0 * age_scale;
        p.pos.x = body_y.cos() * 5.0 * age_scale;
        p.rot.y += body_y;
        p.rot.x += body_y;
    }
    let ease = 1.0 - (1.0 - t).powi(4);
    let aa = (ease * PI).sin();
    let bb = (t * PI).sin() * -(head_x - 0.7) * 0.75;
    let arm = if a.swing_left {
        Slot::LeftArm
    } else {
        Slot::RightArm
    };
    if let Some(p) = r.p(arm) {
        p.rot.x -= aa * 1.2 + bb;
        p.rot.y += body_y * 2.0;
        p.rot.z += (t * PI).sin() * -0.4;
    }
}

fn swing_weapon_down(r: &mut R, a: &AnimIn) {
    let attack2 = sin(a.attack * PI);
    let attack = sin((1.0 - (1.0 - a.attack) * (1.0 - a.attack)) * PI);
    r.rz(Slot::RightArm, 0.0);
    r.rz(Slot::LeftArm, 0.0);
    r.ry(Slot::RightArm, PI / 20.0);
    r.ry(Slot::LeftArm, -PI / 20.0);
    if a.main_right {
        r.rx(
            Slot::RightArm,
            -1.8849558 + cos(a.age * 0.09) * 0.15 + attack2 * 2.2 - attack * 0.4,
        );
        r.rx(
            Slot::LeftArm,
            cos(a.age * 0.19) * 0.5 + attack2 * 1.2 - attack * 0.4,
        );
    } else {
        r.rx(
            Slot::RightArm,
            cos(a.age * 0.19) * 0.5 + attack2 * 1.2 - attack * 0.4,
        );
        r.rx(
            Slot::LeftArm,
            -1.8849558 + cos(a.age * 0.09) * 0.15 + attack2 * 2.2 - attack * 0.4,
        );
    }
    bob_arms(r, a.age);
}

fn swim(r: &mut R, a: &AnimIn) {
    let sa = a.swim_amount;
    if !a.fall_flying {
        let hx = r.rotation(Slot::Head).x;
        r.rx(Slot::Head, lerp(sa, hx, -FRAC_PI_4));
    }
    let swim_pos = a.walk_pos % 26.0;
    let quad = |x: f32| -65.0 * x + x * x;
    if a.use_ticks <= 0.0 {
        for (arm, sign) in [(Slot::LeftArm, 1.0f32), (Slot::RightArm, -1.0)] {
            let cur = r.rotation(arm);
            let spear = (arm == Slot::RightArm && a.right_arm == ArmPose::Spear)
                || (arm == Slot::LeftArm && a.left_arm == ArmPose::Spear);
            let amount = if spear { 0.0 } else { sa };
            let (x, z) = if swim_pos < 14.0 {
                (0.0, PI + sign * 1.8707964 * quad(swim_pos) / quad(14.0))
            } else if swim_pos < 22.0 {
                let t = (swim_pos - 14.0) / 8.0;
                (
                    FRAC_PI_2 * t,
                    if sign > 0.0 {
                        5.012389 - 1.8707964 * t
                    } else {
                        1.2707963 + 1.8707964 * t
                    },
                )
            } else if swim_pos < 26.0 {
                let t = (swim_pos - 22.0) / 4.0;
                (FRAC_PI_2 - FRAC_PI_2 * t, PI)
            } else {
                (cur.x, cur.z)
            };
            r.rot(
                arm,
                Some(lerp(amount, cur.x, x)),
                Some(lerp(amount, cur.y, PI)),
                Some(lerp(amount, cur.z, z)),
            );
        }
    }
    let lx = r.rotation(Slot::LeftLeg).x;
    let rx = r.rotation(Slot::RightLeg).x;
    r.rx(
        Slot::LeftLeg,
        lerp(sa, lx, 0.3 * cos(a.walk_pos * 0.33333334 + PI)),
    );
    r.rx(
        Slot::RightLeg,
        lerp(sa, rx, 0.3 * cos(a.walk_pos * 0.33333334)),
    );
}

fn drowned_swim(r: &mut R, a: &AnimIn) {
    let sa = a.swim_amount;
    let wobble = sin(0.1 * a.age);
    let rx = r.rotation(Slot::RightArm);
    let lx = r.rotation(Slot::LeftArm);
    r.rot(
        Slot::RightArm,
        Some(lerp(sa, rx.x, -PI * 4.0 / 5.0) + sa * 0.35 * wobble),
        None,
        Some(lerp(sa, rx.z, -0.15)),
    );
    r.rot(
        Slot::LeftArm,
        Some(lerp(sa, lx.x, -PI * 4.0 / 5.0) - sa * 0.35 * wobble),
        None,
        Some(lerp(sa, lx.z, 0.15)),
    );
    r.add_rot(Slot::LeftLeg, Vec3::new(-sa * 0.55 * wobble, 0.0, 0.0));
    r.add_rot(Slot::RightLeg, Vec3::new(sa * 0.55 * wobble, 0.0, 0.0));
    r.rx(Slot::Head, 0.0);
}

pub(crate) fn illager(r: &mut R, a: &AnimIn) {
    let head_x = a.head_pitch * DEG;
    let head_y = a.head_yaw * DEG;
    r.rot(Slot::Head, Some(head_x), Some(head_y), None);
    let (pos, sp) = (a.walk_pos, a.walk_speed);
    if a.passenger {
        for arm in [Slot::RightArm, Slot::LeftArm] {
            r.rot(arm, Some(-PI / 5.0), Some(0.0), Some(0.0));
        }
        r.rot(
            Slot::RightLeg,
            Some(-1.4137167),
            Some(PI / 10.0),
            Some(0.07853982),
        );
        r.rot(
            Slot::LeftLeg,
            Some(-1.4137167),
            Some(-PI / 10.0),
            Some(-0.07853982),
        );
    } else {
        r.rot(
            Slot::RightArm,
            Some(cos(pos * 0.6662 + PI) * 2.0 * sp * 0.5),
            Some(0.0),
            Some(0.0),
        );
        r.rot(
            Slot::LeftArm,
            Some(cos(pos * 0.6662) * 2.0 * sp * 0.5),
            Some(0.0),
            Some(0.0),
        );
        r.rot(
            Slot::RightLeg,
            Some(cos(pos * 0.6662) * 1.4 * sp * 0.5),
            Some(0.0),
            Some(0.0),
        );
        r.rot(
            Slot::LeftLeg,
            Some(cos(pos * 0.6662 + PI) * 1.4 * sp * 0.5),
            Some(0.0),
            Some(0.0),
        );
    }
    match a.illager {
        IllagerPose::Attacking => {
            if a.main_hand_empty {
                zombie_arms(
                    r,
                    &AnimIn {
                        aggressive: true,
                        baby: false,
                        right_arm: ArmPose::Empty,
                        left_arm: ArmPose::Empty,
                        ..*a
                    },
                );
                bob_arms(r, a.age);
            } else {
                swing_weapon_down(r, a);
            }
        }
        IllagerPose::Spellcasting => {
            for (arm, side) in [(Slot::RightArm, -1.0f32), (Slot::LeftArm, 1.0)] {
                if let Some(p) = r.p(arm) {
                    p.pos.z = 0.0;
                    p.pos.x = 5.0 * side;
                }
                r.rot(
                    arm,
                    Some(cos(a.age * 0.6662) * 0.25),
                    Some(0.0),
                    Some(-side * PI * 3.0 / 4.0),
                );
            }
        }
        IllagerPose::BowAndArrow => {
            r.ry(Slot::RightArm, -0.1 + head_y);
            r.rx(Slot::RightArm, -FRAC_PI_2 + head_x);
            r.rx(Slot::LeftArm, -0.9424779 + head_x);
            r.ry(Slot::LeftArm, head_y - 0.4);
            r.rz(Slot::LeftArm, FRAC_PI_2);
        }
        IllagerPose::CrossbowHold => crossbow_hold(r, head_x, head_y, true),
        IllagerPose::CrossbowCharge => crossbow_charge(r, a, true),
        IllagerPose::Celebrating => {
            if let Some(p) = r.p(Slot::RightArm) {
                p.pos.z = 0.0;
                p.pos.x = -5.0;
            }
            r.rot(
                Slot::RightArm,
                Some(cos(a.age * 0.6662) * 0.05),
                Some(0.0),
                Some(2.670354),
            );
            if let Some(p) = r.p(Slot::LeftArm) {
                p.pos.z = 0.0;
                p.pos.x = 5.0;
            }
            r.rot(
                Slot::LeftArm,
                Some(cos(a.age * 0.6662) * 0.05),
                Some(0.0),
                Some(-PI * 3.0 / 4.0),
            );
        }
        IllagerPose::Crossed | IllagerPose::Neutral => {}
    }
    let crossed = a.illager == IllagerPose::Crossed;
    let hide = |r: &mut R, s: Slot, hidden: bool| {
        if hidden && let Some(p) = r.p(s) {
            p.scale = Vec3::ZERO;
        }
    };
    hide(r, Slot::Arms, !crossed);
    hide(r, Slot::LeftArm, crossed);
    hide(r, Slot::RightArm, crossed);
    let _ = (Pose::IDENTITY, progress(0.0, 0.0, 1.0));
}
