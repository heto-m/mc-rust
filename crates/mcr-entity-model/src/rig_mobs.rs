use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};

use glam::{Quat, Vec3};

use crate::rig::{AnimIn, DEG, HumanoidStyle, R, Slot, cos, lerp};

fn sin(x: f32) -> f32 {
    x.sin()
}

fn legs4(r: &mut R, a: &AnimIn, k: f32) {
    let swing = a.walk_pos * 0.6662;
    let k = k * a.walk_speed;
    r.rx(Slot::RightHind, cos(swing) * k);
    r.rx(Slot::LeftHind, cos(swing + PI) * k);
    r.rx(Slot::RightFront, cos(swing + PI) * k);
    r.rx(Slot::LeftFront, cos(swing) * k);
}

/// `Mth.triangleWave`.
pub(crate) fn triangle_wave(index: f32, period: f32) -> f32 {
    ((index % period - period * 0.5).abs() - period * 0.25) / (period * 0.25)
}

fn body_roll(a: &AnimIn, offset: f32) -> f32 {
    let progress = ((a.shake + offset) / 1.8).clamp(0.0, 1.0);
    sin(progress * PI) * sin(progress * PI * 11.0) * 0.15 * PI
}

pub(crate) fn wolf(r: &mut R, a: &AnimIn) {
    let adult = r.b.get(Slot::RealHead).is_some();
    r.look(a);
    let scale = if a.baby { 0.5 } else { 1.0 };
    r.ry(
        Slot::Tail,
        if a.aggressive {
            0.0
        } else {
            cos(a.walk_pos * 0.6662) * 1.4 * a.walk_speed
        },
    );
    if a.sitting {
        r.add_pos(Slot::Body, Vec3::new(0.0, 4.0 * scale, -2.0 * scale));
        r.rx(Slot::Body, FRAC_PI_4);
        r.add_pos(Slot::Tail, Vec3::new(0.0, 9.0 * scale, -2.0 * scale));
        for s in [Slot::RightHind, Slot::LeftHind] {
            r.add_pos(s, Vec3::new(0.0, 6.7 * scale, -5.0 * scale));
            r.rx(s, PI * 1.5);
        }
        r.rx(Slot::RightFront, 5.811947);
        r.rx(Slot::LeftFront, 5.811947);
        r.add_pos(Slot::RightFront, Vec3::new(0.01 * scale, scale, 0.0));
        r.add_pos(Slot::LeftFront, Vec3::new(-0.01 * scale, scale, 0.0));
        if adult {
            r.add_pos(Slot::UpperBody, Vec3::new(0.0, 2.0, 0.0));
            r.rx(Slot::UpperBody, PI * 2.0 / 5.0);
            r.ry(Slot::UpperBody, 0.0);
        } else {
            r.add_rot(Slot::Body, Vec3::new(-FRAC_PI_2, 0.0, 0.0));
        }
    } else {
        legs4(r, a, 1.4);
    }
    r.rz(Slot::Body, body_roll(a, -0.16));
    if adult {
        r.rz(Slot::RealHead, a.head_roll + body_roll(a, 0.0));
        r.rz(Slot::UpperBody, body_roll(a, -0.08));
        r.rz(Slot::RealTail, body_roll(a, -0.2));
    } else {
        r.rz(Slot::Head, a.head_roll + body_roll(a, 0.0));
        r.rz(Slot::Tail, body_roll(a, -0.2));
    }
    r.rx(Slot::Tail, a.tail_angle);
}

pub(crate) fn slime(r: &mut R, a: &AnimIn) {
    let squish = a.squish.max(0.0);
    for (i, s) in [
        Slot::Cube0,
        Slot::Cube1,
        Slot::Cube2,
        Slot::Cube3,
        Slot::Cube4,
        Slot::Cube5,
        Slot::Cube6,
        Slot::Cube7,
    ]
    .into_iter()
    .enumerate()
    {
        if r.b.get(s).is_some() {
            let mut p = r.pos(s);
            p.y = -(4.0 - i as f32) * squish * 1.7;
            r.set_pos(s, p);
        }
    }
}

pub(crate) fn fish(r: &mut R, a: &AnimIn) {
    if r.b.get(Slot::BodyBack).is_some() {
        let (amp, ang) = if a.in_water { (1.0, 1.0) } else { (1.3, 1.7) };
        r.ry(Slot::BodyBack, -amp * 0.25 * sin(ang * 0.6 * a.age));
    } else if r.b.get(Slot::TailFin).is_some() {
        let amp = if a.in_water { 1.0 } else { 1.5 };
        r.ry(Slot::TailFin, -amp * 0.45 * sin(0.6 * a.age));
    } else {
        let amp = if a.in_water { 1.0 } else { 1.5 };
        r.ry(Slot::Tail, -amp * 0.45 * sin(0.6 * a.age));
    }
}

pub(crate) fn pufferfish(r: &mut R, a: &AnimIn) {
    let wave = 0.4 * sin(a.age * 0.2);
    for (right, left) in [
        (Slot::RightFin, Slot::LeftFin),
        (Slot::RightBlueFin, Slot::LeftBlueFin),
    ] {
        r.rz(right, -0.2 + wave);
        r.rz(left, 0.2 - wave);
    }
}

pub(crate) fn squid(r: &mut R, a: &AnimIn) {
    for s in [
        Slot::Tentacle0,
        Slot::Tentacle1,
        Slot::Tentacle2,
        Slot::Tentacle3,
        Slot::Tentacle4,
        Slot::Tentacle5,
        Slot::Tentacle6,
        Slot::Tentacle7,
    ] {
        r.rx(s, a.tentacle);
    }
}

pub(crate) fn blaze(r: &mut R, a: &AnimIn) {
    let parts = [
        Slot::Part0,
        Slot::Part1,
        Slot::Part2,
        Slot::Part3,
        Slot::Part4,
        Slot::Part5,
        Slot::Part6,
        Slot::Part7,
        Slot::Part8,
        Slot::Part9,
        Slot::Part10,
        Slot::Part11,
    ];
    let mut angle = a.age * PI * -0.1;
    for (i, s) in parts.iter().enumerate().take(4) {
        r.set_pos(
            *s,
            Vec3::new(
                cos(angle) * 9.0,
                -2.0 + cos((i as f32 * 2.0 + a.age) * 0.25),
                sin(angle) * 9.0,
            ),
        );
        angle += FRAC_PI_2;
    }
    angle = FRAC_PI_4 + a.age * PI * 0.03;
    for (i, s) in parts.iter().enumerate().take(8).skip(4) {
        r.set_pos(
            *s,
            Vec3::new(
                cos(angle) * 7.0,
                2.0 + cos((i as f32 * 2.0 + a.age) * 0.25),
                sin(angle) * 7.0,
            ),
        );
        angle += FRAC_PI_2;
    }
    angle = 0.47123894 + a.age * PI * -0.05;
    for (i, s) in parts.iter().enumerate().skip(8) {
        r.set_pos(
            *s,
            Vec3::new(
                cos(angle) * 5.0,
                11.0 + cos((i as f32 * 1.5 + a.age) * 0.5),
                sin(angle) * 5.0,
            ),
        );
        angle += FRAC_PI_2;
    }
    r.look(a);
}

pub(crate) fn ghast(r: &mut R, a: &AnimIn) {
    for (i, s) in [
        Slot::Tentacle0,
        Slot::Tentacle1,
        Slot::Tentacle2,
        Slot::Tentacle3,
        Slot::Tentacle4,
        Slot::Tentacle5,
        Slot::Tentacle6,
        Slot::Tentacle7,
        Slot::Tentacle8,
    ]
    .into_iter()
    .enumerate()
    {
        r.rx(s, 0.2 * sin(a.age * 0.3 + i as f32) + 0.4);
    }
}

pub(crate) fn enderman(r: &mut R, a: &AnimIn) {
    crate::rig_humanoid::humanoid(r, a, HumanoidStyle::Plain);
    for s in [Slot::RightArm, Slot::LeftArm, Slot::RightLeg, Slot::LeftLeg] {
        let x = r.rotation(s).x * 0.5;
        r.rx(s, x.clamp(-0.4, 0.4));
    }
    if a.carrying {
        r.rx(Slot::RightArm, -0.5);
        r.rx(Slot::LeftArm, -0.5);
        r.rz(Slot::RightArm, 0.05);
        r.rz(Slot::LeftArm, -0.05);
    }
    if a.creepy {
        r.add_pos(Slot::Head, Vec3::new(0.0, -5.0, 0.0));
        r.add_pos(Slot::Hat, Vec3::new(0.0, 5.0, 0.0));
    }
}

pub(crate) fn snow_golem(r: &mut R, a: &AnimIn) {
    let yaw = a.head_yaw * DEG;
    r.rot(Slot::Head, Some(a.head_pitch * DEG), Some(yaw), None);
    let upper = yaw * 0.25;
    r.ry(Slot::UpperBody, upper);
    let (s, c) = (sin(upper), cos(upper));
    r.ry(Slot::LeftArm, upper);
    r.ry(Slot::RightArm, upper + PI);
    let y_left = r.pos(Slot::LeftArm).y;
    let y_right = r.pos(Slot::RightArm).y;
    r.set_pos(Slot::LeftArm, Vec3::new(c * 5.0, y_left, -s * 5.0));
    r.set_pos(Slot::RightArm, Vec3::new(-c * 5.0, y_right, s * 5.0));
}

pub(crate) fn iron_golem(r: &mut R, a: &AnimIn) {
    let tw = |x: f32, p: f32| triangle_wave(x, p);
    if a.attack_ticks > 0.0 {
        let x = -2.0 + 1.5 * tw(a.attack_ticks, 10.0);
        r.rx(Slot::RightArm, x);
        r.rx(Slot::LeftArm, x);
    } else if a.offer_ticks > 0.0 {
        r.rx(Slot::RightArm, -0.8 + 0.025 * tw(a.offer_ticks, 70.0));
        r.rx(Slot::LeftArm, 0.0);
    } else {
        let t = tw(a.walk_pos, 13.0);
        r.rx(Slot::RightArm, (-0.2 + 1.5 * t) * a.walk_speed);
        r.rx(Slot::LeftArm, (-0.2 - 1.5 * t) * a.walk_speed);
    }
    r.rot(
        Slot::Head,
        Some(a.head_pitch * DEG),
        Some(a.head_yaw * DEG),
        None,
    );
    let t = tw(a.walk_pos, 13.0);
    r.rot(
        Slot::RightLeg,
        Some(-1.5 * t * a.walk_speed),
        Some(0.0),
        None,
    );
    r.rot(Slot::LeftLeg, Some(1.5 * t * a.walk_speed), Some(0.0), None);
}

pub(crate) fn guardian(r: &mut R, a: &AnimIn) {
    r.look(a);
    let spikes = [
        Slot::Spike0,
        Slot::Spike1,
        Slot::Spike2,
        Slot::Spike3,
        Slot::Spike4,
        Slot::Spike5,
        Slot::Spike6,
        Slot::Spike7,
        Slot::Spike8,
        Slot::Spike9,
        Slot::Spike10,
        Slot::Spike11,
    ];
    let withdrawal = (1.0 - a.holding) * 0.55;
    for (i, s) in spikes.into_iter().enumerate() {
        if r.b.get(s).is_none() {
            continue;
        }
        let rest = r.pos(s);
        let rest_k = 1.0 + cos(i as f32) * 0.01;
        let k = 1.0 + cos(a.age * 1.5 + i as f32) * 0.01 - withdrawal;
        let base = Vec3::new(rest.x, rest.y - 16.0, rest.z) / rest_k;
        r.set_pos(s, Vec3::new(base.x * k, 16.0 + base.y * k, base.z * k));
    }
    let swim = a.tentacle;
    r.ry(Slot::Tail0, sin(swim) * PI * 0.05);
    r.ry(Slot::Tail1, sin(swim) * PI * 0.1);
    r.ry(Slot::Tail2, sin(swim) * PI * 0.15);
}

pub(crate) fn phantom(r: &mut R, a: &AnimIn) {
    let anim = a.flap * 7.448451 * DEG;
    let wing = cos(anim) * 16.0 * DEG;
    r.rz(Slot::LeftWingBase, wing);
    r.rz(Slot::LeftWingTip, wing);
    r.rz(Slot::RightWingBase, -wing);
    r.rz(Slot::RightWingTip, -wing);
    let tail = -(5.0 + cos(anim * 2.0) * 5.0) * DEG;
    r.rx(Slot::TailBase, tail);
    r.rx(Slot::TailTip, tail);
}

pub(crate) fn dragon(r: &mut R, a: &AnimIn) {
    let flap = a.flap * PI * 2.0;
    r.rx(Slot::Jaw, (sin(flap) + 1.0) * 0.2);
    let mut bounce = sin(flap - 1.0) + 1.0;
    bounce = (bounce * bounce + bounce * 2.0) * 0.05;
    if let Some(p) = r.p(Slot::Root) {
        p.pos = Vec3::new(0.0, (bounce - 2.0) * 16.0, -48.0);
        p.rot.x = bounce * 2.0 * DEG;
    }
    let necks = [
        Slot::Neck0,
        Slot::Neck1,
        Slot::Neck2,
        Slot::Neck3,
        Slot::Neck4,
    ];
    let mut at = r.pos(Slot::Neck0);
    for (i, s) in necks.into_iter().enumerate() {
        let x_rot = cos(i as f32 * 0.45 + flap) * 0.15;
        if let Some(p) = r.p(s) {
            p.rot = Vec3::new(x_rot, 0.0, 0.0);
            p.pos = at;
        }
        at.y += sin(x_rot) * 10.0;
        at.z -= cos(x_rot) * 10.0;
    }
    r.set_pos(Slot::Head, at);
    r.rot(Slot::Head, Some(0.0), Some(0.0), Some(0.0));
    let lw = [0.125 - cos(flap) * 0.2, -0.25, -(sin(flap) + 0.125) * 0.8];
    let lt = (sin(flap + 2.0) + 0.5) * 0.75;
    r.rot(Slot::LeftWing, Some(lw[0]), Some(lw[1]), Some(lw[2]));
    r.rz(Slot::LeftWingTip, lt);
    r.rot(Slot::RightWing, Some(lw[0]), Some(-lw[1]), Some(-lw[2]));
    r.rz(Slot::RightWingTip, -lt);
    let k = bounce * 0.1;
    for (front, front_tip, front_foot, hind, hind_tip, hind_foot) in [
        (
            Slot::LeftFront,
            Slot::LeftFrontLegTip,
            Slot::LeftFrontFoot,
            Slot::LeftHind,
            Slot::LeftHindLegTip,
            Slot::LeftHindFoot,
        ),
        (
            Slot::RightFront,
            Slot::RightFrontLegTip,
            Slot::RightFrontFoot,
            Slot::RightHind,
            Slot::RightHindLegTip,
            Slot::RightHindFoot,
        ),
    ] {
        r.rx(hind, 1.0 + k);
        r.rx(hind_tip, 0.5 + k);
        r.rx(hind_foot, 0.75 + k);
        r.rx(front, 1.3 + k);
        r.rx(front_tip, -0.5 - k);
        r.rx(front_foot, 0.75 + k);
    }
    let tails = [
        Slot::Tail0,
        Slot::Tail1,
        Slot::Tail2,
        Slot::Tail3,
        Slot::Tail4,
        Slot::Tail5,
        Slot::Tail6,
        Slot::Tail7,
        Slot::Tail8,
        Slot::Tail9,
        Slot::Tail10,
        Slot::Tail11,
    ];
    let mut at = r.pos(Slot::Tail0);
    let mut tail_x = 0.0;
    for (i, s) in tails.into_iter().enumerate() {
        tail_x += sin(i as f32 * 0.45 + flap) * 0.05;
        if let Some(p) = r.p(s) {
            p.rot = Vec3::new(tail_x, PI, 0.0);
            p.pos = at;
        }
        at.y += sin(tail_x) * 10.0;
        at.z += cos(tail_x) * 10.0;
    }
}

pub(crate) fn wither(r: &mut R, a: &AnimIn) {
    for s in [Slot::RightHead, Slot::LeftHead] {
        r.rot(s, Some(0.0), Some(0.0), None);
    }
    let wave = cos(a.age * 0.1);
    let rib = (0.065 + 0.05 * wave) * PI;
    r.rx(Slot::Ribcage, rib);
    r.set_pos(
        Slot::Tail,
        Vec3::new(-2.0, 6.9 + cos(rib) * 10.0, -0.5 + sin(rib) * 10.0),
    );
    r.rx(Slot::Tail, (0.265 + 0.1 * wave) * PI);
    r.rot(
        Slot::CenterHead,
        Some(a.head_pitch * DEG),
        Some(a.head_yaw * DEG),
        None,
    );
}

pub(crate) fn equine(r: &mut R, a: &AnimIn) {
    let clamped = a.head_yaw.clamp(-20.0, 20.0);
    let mut head_x = a.head_pitch * DEG;
    if a.walk_speed > 0.2 {
        head_x += cos(a.walk_pos * 0.8) * 0.15 * a.walk_speed;
    }
    let (eating, standing, feeding) = (a.eat, a.stand, a.feed);
    let i_standing = 1.0 - standing;
    let water = if a.in_water { 0.2 } else { 1.0 };
    let leg_anim = cos(water * a.walk_pos * 0.6662 + PI);
    let leg_x = leg_anim * 0.8 * a.walk_speed;
    let rest = 1.0 - standing.max(eating);
    let base_head = rest * (PI / 6.0 + head_x + feeding * sin(a.age) * 0.05);
    let eat_x = if a.baby && a.donkey {
        FRAC_PI_2
    } else {
        2.1816616
    };
    let head_x_rot =
        standing * (PI / 12.0 + head_x) + eating * (eat_x + sin(a.age) * 0.05) + base_head;
    let head_y_rot = standing * clamped * DEG + rest * (clamped * DEG);
    r.rot(Slot::HeadParts, Some(head_x_rot), Some(head_y_rot), None);
    let (leg_y_off, leg_z_off, stand_x, stand_angle, tail_off) = match (a.baby, a.donkey) {
        (false, _) => (12.0, 4.0, -PI / 3.0, PI / 12.0, 0.0),
        (true, false) => (4.0, 0.0, -PI / 3.0, PI / 12.0, -PI / 2.0),
        (true, true) => (1.0, 0.5, 0.0, PI / 3.0, -PI / 4.0),
    };
    if let Some(p) = r.p(Slot::HeadParts) {
        match (a.baby, a.donkey) {
            (false, _) => {
                p.pos.y += lerp(eating, lerp(standing, 0.0, -8.0), 7.0);
                p.pos.z = lerp(standing, p.pos.z, -4.0);
            }
            (true, false) => {
                p.pos.y += lerp(eating, lerp(standing, 0.0, -2.0), 2.0);
                p.pos.z = lerp(standing, p.pos.z, -4.0);
            }
            (true, true) => {
                p.pos.y = lerp(eating, p.pos.y, -1.2);
                p.pos.z = lerp(standing, p.pos.z, -3.6);
            }
        }
    }
    let body_x = r.rotation(Slot::Body).x;
    r.rx(Slot::Body, standing * -FRAC_PI_4 + i_standing * body_x);
    let (front_y, front_z) = {
        let p = r.pos(Slot::LeftFront);
        (p.y - leg_y_off * standing, p.z + leg_z_off * standing)
    };
    for s in [Slot::LeftFront, Slot::RightFront] {
        if let Some(p) = r.p(s) {
            p.pos.y = front_y;
            p.pos.z = front_z;
        }
    }
    let stand = stand_angle * standing;
    let bob = cos(a.age * 0.6 + PI);
    let rleg = (stand_x + bob) * standing + leg_x * i_standing;
    let lleg = (stand_x - bob) * standing - leg_x * i_standing;
    r.rx(
        Slot::LeftHind,
        stand - leg_anim * 0.5 * a.walk_speed * i_standing,
    );
    r.rx(
        Slot::RightHind,
        stand + leg_anim * 0.5 * a.walk_speed * i_standing,
    );
    if a.baby && a.donkey {
        for s in [Slot::LeftHind, Slot::RightHind] {
            let mut p = r.pos(s);
            p.y = lerp(standing, p.y, -0.3);
            r.set_pos(s, p);
        }
    }
    r.rx(Slot::LeftFront, rleg);
    r.rx(Slot::RightFront, lleg);
    let scale = if a.baby { 0.5 } else { 1.0 };
    r.rx(Slot::Tail, tail_off + PI / 6.0 + a.walk_speed * 0.75);
    r.add_pos(
        Slot::Tail,
        Vec3::new(0.0, a.walk_speed * scale, a.walk_speed * 2.0 * scale),
    );
    r.ry(Slot::Tail, if a.charging { cos(a.age * 0.7) } else { 0.0 });
}

pub(crate) fn llama(r: &mut R, a: &AnimIn) {
    r.look(a);
    legs4(r, a, 1.4);
}

pub(crate) fn strider(r: &mut R, a: &AnimIn) {
    let pos = a.walk_pos;
    let sp = a.walk_speed.min(0.25);
    if !a.passenger {
        r.rot(
            Slot::Body,
            Some(a.head_pitch * DEG),
            Some(a.head_yaw * DEG),
            None,
        );
    } else {
        r.rot(Slot::Body, Some(0.0), Some(0.0), None);
    }
    r.rz(Slot::Body, 0.1 * sin(pos * 1.5) * 4.0 * sp);
    r.rx(Slot::LeftLeg, sin(pos * 1.5 * 0.5) * 2.0 * sp);
    r.rx(Slot::RightLeg, sin(pos * 1.5 * 0.5 + PI) * 2.0 * sp);
    r.rz(Slot::LeftLeg, PI / 18.0 * cos(pos * 1.5 * 0.5) * sp);
    r.rz(Slot::RightLeg, PI / 18.0 * cos(pos * 1.5 * 0.5 + PI) * sp);
    let flow = cos(pos * 1.5 + PI) * sp;
    if a.baby {
        for (s, k, wave) in [
            (Slot::Bristle2, 0.6, 0.1 * sin(a.age * 0.4)),
            (Slot::Bristle1, 1.2, 0.1 * sin(a.age * 0.2)),
            (Slot::Bristle0, 1.3, 0.05 * sin(a.age * -0.4)),
        ] {
            r.add_rot(s, Vec3::new(flow * k + wave, 0.0, 0.0));
        }
        let mut body = r.pos(Slot::Body);
        body.y = 17.25 - cos(pos * 1.5) * 2.0 * sp;
        r.set_pos(Slot::Body, body);
        let mut l = r.pos(Slot::LeftLeg);
        l.y = 20.0 + 2.0 * sin(pos * 1.5 * 0.5 + PI) * 2.0 * sp;
        r.set_pos(Slot::LeftLeg, l);
        let mut rr = r.pos(Slot::RightLeg);
        rr.y = 20.0 + 2.0 * sin(pos * 1.5 * 0.5) * 2.0 * sp;
        r.set_pos(Slot::RightLeg, rr);
        return;
    }
    r.rz(Slot::RightBottomBristle, -1.2217305);
    r.rz(Slot::RightMiddleBristle, -1.134464);
    r.rz(Slot::RightTopBristle, -0.87266463);
    r.rz(Slot::LeftTopBristle, 0.87266463);
    r.rz(Slot::LeftMiddleBristle, 1.134464);
    r.rz(Slot::LeftBottomBristle, 1.2217305);
    for (top, mid, bottom) in [
        (
            Slot::RightTopBristle,
            Slot::RightMiddleBristle,
            Slot::RightBottomBristle,
        ),
        (
            Slot::LeftTopBristle,
            Slot::LeftMiddleBristle,
            Slot::LeftBottomBristle,
        ),
    ] {
        for (s, k, wave) in [
            (top, 0.6, 0.1 * sin(a.age * 0.4)),
            (mid, 1.2, 0.1 * sin(a.age * 0.2)),
            (bottom, 1.3, 0.05 * sin(a.age * -0.4)),
        ] {
            r.add_rot(s, Vec3::new(0.0, 0.0, flow * k + wave));
        }
    }
    let mut body = r.pos(Slot::Body);
    body.y = 2.0 - 2.0 * cos(pos * 1.5) * 2.0 * sp;
    r.set_pos(Slot::Body, body);
    let mut l = r.pos(Slot::LeftLeg);
    l.y = 8.0 + 2.0 * sin(pos * 1.5 * 0.5 + PI) * 2.0 * sp;
    r.set_pos(Slot::LeftLeg, l);
    let mut rr = r.pos(Slot::RightLeg);
    rr.y = 8.0 + 2.0 * sin(pos * 1.5 * 0.5) * 2.0 * sp;
    r.set_pos(Slot::RightLeg, rr);
}

pub(crate) fn hoglin(r: &mut R, a: &AnimIn) {
    let (sp, pos) = (a.walk_speed, a.walk_pos);
    r.rz(Slot::RightEar, -PI * 2.0 / 9.0 - sp * sin(pos));
    r.rz(Slot::LeftEar, PI * 2.0 / 9.0 + sp * sin(pos));
    r.ry(Slot::Head, a.head_yaw * DEG);
    let f = 1.0 - (10.0 - 2.0 * a.attack_ticks).abs() / 10.0;
    r.rx(Slot::Head, lerp(f, 0.87266463, -PI / 9.0));
    if a.baby {
        r.add_pos(Slot::Head, Vec3::new(0.0, f * 2.5, 0.0));
    }
    let rf = cos(pos) * 1.2 * sp;
    let lf = cos(pos + PI) * 1.2 * sp;
    r.rx(Slot::RightFront, rf);
    r.rx(Slot::LeftFront, lf);
    r.rx(Slot::RightHind, lf);
    r.rx(Slot::LeftHind, rf);
}

pub(crate) fn ravager(r: &mut R, a: &AnimIn) {
    let attack = a.attack_ticks;
    if attack > 0.0 {
        let head_anim = triangle_wave(attack, 10.0);
        let scaled = (1.0 + head_anim) * 0.5;
        let head_pos = scaled * scaled * scaled * 12.0;
        let y_off = head_pos * sin(r.rotation(Slot::Neck).x);
        if let Some(p) = r.p(Slot::Neck) {
            p.pos.z = -6.5 + head_pos;
            p.pos.y = -7.0 - y_off;
        }
        if attack > 5.0 {
            r.rx(Slot::Mouth, sin((-4.0 + attack) / 4.0) * PI * 0.4);
        } else {
            r.rx(Slot::Mouth, PI / 20.0 * sin(PI * attack / 10.0));
        }
    } else {
        let y_off = -sin(r.rotation(Slot::Neck).x);
        let stunned = a.stunned > 0.0;
        if let Some(p) = r.p(Slot::Neck) {
            p.pos = Vec3::new(0.0, -7.0 - y_off, 5.5);
            p.rot.x = if stunned { 0.21991149 } else { 0.0 };
            if stunned {
                p.pos.x = sin(a.stunned / 40.0 * 10.0) * 3.0;
            }
        }
        r.rx(Slot::Mouth, PI * if stunned { 0.05 } else { 0.01 });
        if !stunned && a.roar > 0.0 {
            r.rx(Slot::Mouth, FRAC_PI_2 * sin(a.roar * PI * 0.25));
        }
    }
    r.rot(
        Slot::Head,
        Some(a.head_pitch * DEG),
        Some(a.head_yaw * DEG),
        None,
    );
    legs4(r, a, 0.4);
}

pub(crate) fn shulker(r: &mut R, a: &AnimIn) {
    let bs = (0.5 + a.peek) * PI;
    let q = -1.0 + sin(bs);
    let extra = if bs > PI { sin(a.age * 0.1) * 0.7 } else { 0.0 };
    r.set_pos(Slot::Lid, Vec3::new(0.0, 16.0 + sin(bs) * 8.0 + extra, 0.0));
    r.ry(
        Slot::Lid,
        if a.peek > 0.3 {
            q * q * q * q * PI * 0.125
        } else {
            0.0
        },
    );
    r.rot(
        Slot::Head,
        Some(a.head_pitch * DEG),
        Some(a.head_yaw * DEG),
        None,
    );
}

pub(crate) fn vex(r: &mut R, a: &AnimIn) {
    r.rot(
        Slot::Head,
        Some(a.head_pitch * DEG),
        Some(a.head_yaw * DEG),
        None,
    );
    let bob = cos(a.age * 5.5 * DEG) * 0.1;
    r.rz(Slot::RightArm, PI / 5.0 + bob);
    r.rz(Slot::LeftArm, -(PI / 5.0 + bob));
    if a.charging {
        r.rx(Slot::Body, 0.0);
        let (right, left) = (a.item_held && a.main_right, a.item_held && !a.main_right);
        if !right && !left {
            r.rot(
                Slot::RightArm,
                Some(-1.2217305),
                Some(PI / 12.0),
                Some(-0.47123888 - bob),
            );
            r.rot(
                Slot::LeftArm,
                Some(-1.2217305),
                Some(-PI / 12.0),
                Some(0.47123888 + bob),
            );
        } else {
            if right {
                r.rot(
                    Slot::RightArm,
                    Some(PI * 7.0 / 6.0),
                    Some(PI / 12.0),
                    Some(-0.47123888 - bob),
                );
            }
            if left {
                r.rot(
                    Slot::LeftArm,
                    Some(PI * 7.0 / 6.0),
                    Some(-PI / 12.0),
                    Some(0.47123888 + bob),
                );
            }
        }
    } else {
        r.rx(Slot::Body, PI / 20.0);
    }
    let wing = 1.0995574 + cos(a.age * 45.836624 * DEG) * DEG * 16.2;
    r.rot(
        Slot::LeftWing,
        Some(0.47123888),
        Some(wing),
        Some(-0.47123888),
    );
    r.rot(
        Slot::RightWing,
        Some(0.47123888),
        Some(-wing),
        Some(0.47123888),
    );
}

pub(crate) fn allay(r: &mut R, a: &AnimIn) {
    let speed = a.walk_speed;
    let flap_speed = a.age * 20.0 * DEG + a.walk_pos;
    let flap = cos(flap_speed) * PI * 0.15 + speed;
    let idle = a.age * 9.0 * DEG;
    let flying = (speed / 0.3).min(1.0);
    let idle_factor = 1.0 - flying;
    let holding = a.holding;
    if a.charging {
        let dance = a.age * 8.0 * DEG + speed;
        let spin = 0.0;
        r.rz(Slot::InnerRoot, cos(dance) * 16.0 * DEG * (1.0 - spin));
        r.ry(Slot::Head, cos(dance) * 30.0 * DEG * (1.0 - spin));
        r.rz(Slot::Head, cos(dance) * 14.0 * DEG * (1.0 - spin));
    } else {
        r.rot(
            Slot::Head,
            Some(a.head_pitch * DEG),
            Some(a.head_yaw * DEG),
            None,
        );
    }
    r.rot(
        Slot::RightWing,
        Some(0.43633232 * (1.0 - flying)),
        Some(-FRAC_PI_4 + flap),
        None,
    );
    r.rot(
        Slot::LeftWing,
        Some(0.43633232 * (1.0 - flying)),
        Some(FRAC_PI_4 - flap),
        None,
    );
    r.rx(Slot::Body, flying * FRAC_PI_4);
    let arm_x = holding * lerp(flying, -PI / 3.0, -1.134464);
    r.add_pos(
        Slot::InnerRoot,
        Vec3::new(0.0, cos(idle) * 0.25 * idle_factor, 0.0),
    );
    r.rx(Slot::RightArm, arm_x);
    r.rx(Slot::LeftArm, arm_x);
    let arm_idle = idle_factor * (1.0 - holding);
    let amount = 0.43633232 - cos(idle + PI * 1.5) * PI * 0.075 * arm_idle;
    r.rz(Slot::LeftArm, -amount);
    r.rz(Slot::RightArm, amount);
    r.ry(Slot::RightArm, 0.27925268 * holding);
    r.ry(Slot::LeftArm, -0.27925268 * holding);
}

pub(crate) fn parrot(r: &mut R, a: &AnimIn) {
    let flying = !a.on_ground && !a.sitting;
    let head = [
        Slot::Head,
        Slot::Tail,
        Slot::Body,
        Slot::LeftWing,
        Slot::RightWing,
        Slot::LeftLeg,
        Slot::RightLeg,
    ];
    if flying {
        r.add_rot(Slot::LeftLeg, Vec3::new(PI * 2.0 / 9.0, 0.0, 0.0));
        r.add_rot(Slot::RightLeg, Vec3::new(PI * 2.0 / 9.0, 0.0, 0.0));
    }
    if a.sitting {
        for s in head {
            r.add_pos(s, Vec3::new(0.0, 1.9, 0.0));
        }
        r.add_rot(Slot::Tail, Vec3::new(PI / 6.0, 0.0, 0.0));
        r.rz(Slot::LeftWing, -0.0873);
        r.rz(Slot::RightWing, 0.0873);
        r.add_rot(Slot::LeftLeg, Vec3::new(FRAC_PI_2, 0.0, 0.0));
        r.add_rot(Slot::RightLeg, Vec3::new(FRAC_PI_2, 0.0, 0.0));
    }
    r.rot(
        Slot::Head,
        Some(a.head_pitch * DEG),
        Some(a.head_yaw * DEG),
        None,
    );
    if !a.sitting {
        if !flying {
            r.add_rot(
                Slot::LeftLeg,
                Vec3::new(cos(a.walk_pos * 0.6662) * 1.4 * a.walk_speed, 0.0, 0.0),
            );
            r.add_rot(
                Slot::RightLeg,
                Vec3::new(cos(a.walk_pos * 0.6662 + PI) * 1.4 * a.walk_speed, 0.0, 0.0),
            );
        }
        let bob = a.flap * 0.3;
        for s in head {
            r.add_pos(s, Vec3::new(0.0, bob, 0.0));
        }
        r.add_rot(
            Slot::Tail,
            Vec3::new(cos(a.walk_pos * 0.6662) * 0.3 * a.walk_speed, 0.0, 0.0),
        );
        r.rz(Slot::LeftWing, -0.0873 - a.flap);
        r.rz(Slot::RightWing, 0.0873 + a.flap);
    }
}

pub(crate) fn endermite(r: &mut R, a: &AnimIn, silverfish: bool) {
    let segments = [
        Slot::Segment0,
        Slot::Segment1,
        Slot::Segment2,
        Slot::Segment3,
        Slot::Segment4,
        Slot::Segment5,
        Slot::Segment6,
    ];
    let (yk, xk) = if silverfish { (0.05, 0.2) } else { (0.01, 0.1) };
    for (i, s) in segments.into_iter().enumerate() {
        if r.b.get(s).is_none() {
            continue;
        }
        let w = (i as i32 - 2).abs() as f32;
        let phase = a.age * 0.9 + i as f32 * 0.15 * PI;
        r.ry(s, cos(phase) * PI * yk * (1.0 + w));
        let mut p = r.pos(s);
        p.x = sin(phase) * PI * xk * w;
        r.set_pos(s, p);
    }
    if silverfish {
        for (layer, seg) in [
            (Slot::Layer0, Slot::Segment2),
            (Slot::Layer1, Slot::Segment4),
            (Slot::Layer2, Slot::Segment1),
        ] {
            let y = r.rotation(seg).y;
            let x = r.pos(seg).x;
            r.ry(layer, y);
            if layer != Slot::Layer0 {
                let mut p = r.pos(layer);
                p.x = x;
                r.set_pos(layer, p);
            }
        }
    }
}

pub(crate) fn crystal(r: &mut R, a: &AnimIn) {
    if !a.has_chest {
        r.hide(Slot::Base);
    }
    let spin = a.flap * 3.0;
    let hh = sin(a.flap * 0.2) / 2.0 + 0.5;
    let y = ((hh * hh + hh) * 0.4 - 1.4) * 16.0;
    r.add_pos(Slot::OuterGlass, Vec3::new(0.0, y / 2.0, 0.0));
    let axis = Vec3::new(FRAC_PI_4.sin(), 0.0, FRAC_PI_4.sin());
    let tilt = Quat::from_axis_angle(axis, PI / 3.0);
    let ry = Quat::from_rotation_y(spin * DEG);
    r.rotate_by(Slot::OuterGlass, ry * tilt);
    r.rotate_by(Slot::InnerGlass, tilt * ry);
    r.rotate_by(Slot::Cube, tilt * ry);
}

pub(crate) fn dolphin(r: &mut R, a: &AnimIn) {
    let mut x = a.head_pitch * DEG;
    r.ry(Slot::Body, a.head_yaw * DEG);
    if a.walk_speed > 0.01 || a.charging {
        x += -0.05 - 0.05 * cos(a.age * 0.3);
        r.rx(Slot::Tail, -0.1 * cos(a.age * 0.3));
        r.rx(Slot::TailFin, -0.2 * cos(a.age * 0.3));
    }
    r.rx(Slot::Body, x);
}

pub(crate) fn bee(r: &mut R, a: &AnimIn) {
    if !a.has_stinger {
        r.hide(Slot::Stinger);
    }
    if !a.on_ground {
        let speed = a.age * 120.32113 * DEG;
        let z = cos(speed) * PI * 0.15;
        let x = r.rotation(Slot::RightWing).x;
        r.rot(Slot::RightWing, None, Some(0.0), Some(z));
        r.rot(Slot::LeftWing, Some(x), Some(0.0), Some(-z));
        r.rx(Slot::FrontLegs, FRAC_PI_4);
        r.rx(Slot::MiddleLegs, FRAC_PI_4);
        r.rx(Slot::BackLegs, FRAC_PI_4);
    }
    if !a.aggressive && !a.on_ground {
        let speed = cos(a.age * 0.18);
        r.rx(Slot::Bone, 0.1 + speed * PI * 0.025);
        r.add_pos(Slot::Bone, Vec3::new(0.0, -cos(a.age * 0.18) * 0.9, 0.0));
        r.rx(Slot::FrontLegs, -speed * PI * 0.1 + PI / 8.0);
        r.rx(Slot::BackLegs, -speed * PI * 0.05 + PI / 4.0);
    }
    if a.roll > 0.0 {
        let x = r.rotation(Slot::Bone).x;
        r.rx(Slot::Bone, lerp(a.roll, x, 3.0915928));
    }
}

pub(crate) fn turtle(r: &mut R, a: &AnimIn) {
    if a.on_land {
        let lay = if a.charging { 4.0 } else { 1.0 };
        let amp = if a.charging { 2.0 } else { 1.0 };
        let swing = a.walk_pos * 5.0;
        let front = cos(lay * swing);
        let hind = cos(swing);
        r.ry(Slot::RightFront, -front * 8.0 * a.walk_speed * amp);
        r.ry(Slot::LeftFront, front * 8.0 * a.walk_speed * amp);
        r.ry(Slot::RightHind, -hind * 3.0 * a.walk_speed);
        r.ry(Slot::LeftHind, hind * 3.0 * a.walk_speed);
    } else {
        let swing = cos(a.walk_pos * 0.6662 * 0.6) * 0.5 * a.walk_speed;
        r.rx(Slot::RightHind, swing);
        r.rx(Slot::LeftHind, -swing);
        r.rz(Slot::RightFront, -swing);
        r.rz(Slot::LeftFront, swing);
    }
    if a.has_chest {
        r.add_pos(Slot::Root, Vec3::new(0.0, -1.0, 0.0));
    }
}

pub(crate) fn polar_bear(r: &mut R, a: &AnimIn) {
    r.look(a);
    legs4(r, a, 1.4);
    let s = a.stand * a.stand;
    let age = if a.baby { 0.5 } else { 1.0 };
    r.add_rot(Slot::Body, Vec3::new(-s * PI * 0.35, 0.0, 0.0));
    r.add_pos(Slot::Body, Vec3::new(0.0, s * age * 2.0, 0.0));
    let (y, z) = (
        r.pos(Slot::RightFront).y - s * age * 20.0,
        r.pos(Slot::RightFront).z + s * age * 4.0,
    );
    for leg in [Slot::RightFront, Slot::LeftFront] {
        if let Some(p) = r.p(leg) {
            p.pos.y = y;
            p.pos.z = z;
            p.rot.x -= s * PI * 0.45;
        }
    }
    r.add_pos(Slot::Head, Vec3::new(0.0, -s * 24.0, s * 13.0));
    r.add_rot(Slot::Head, Vec3::new(s * PI * 0.15, 0.0, 0.0));
}

pub(crate) fn panda(r: &mut R, a: &AnimIn) {
    r.look(a);
    legs4(r, a, 1.4);
    if a.unhappy {
        let w = 0.35 * sin(0.6 * a.age);
        r.ry(Slot::Head, w);
        r.rz(Slot::Head, w);
        r.rx(Slot::RightFront, -0.75 * sin(0.3 * a.age));
        r.rx(Slot::LeftFront, 0.75 * sin(0.3 * a.age));
    } else {
        r.rz(Slot::Head, 0.0);
    }
    if a.sneeze > 0.0 {
        let t = a.sneeze;
        if t < 15.0 {
            r.rx(Slot::Head, -FRAC_PI_4 * t / 14.0);
        } else if t < 20.0 {
            r.rx(
                Slot::Head,
                -FRAC_PI_4 + FRAC_PI_4 * ((t - 15.0) / 5.0).floor(),
            );
        }
    }
    if a.sit_amount > 0.0 {
        let hx = r.rotation(Slot::Head).x;
        let bx = r.rotation(Slot::Body).x;
        r.rx(Slot::Body, lerp(a.sit_amount, bx, 1.7407963));
        r.rx(Slot::Head, lerp(a.sit_amount, hx, FRAC_PI_2));
        r.rz(Slot::RightFront, -0.27079642);
        r.rz(Slot::LeftFront, 0.27079642);
        r.rz(Slot::RightHind, 0.5707964);
        r.rz(Slot::LeftHind, -0.5707964);
        if a.eat > 0.0 {
            r.rx(Slot::Head, FRAC_PI_2 + 0.2 * sin(a.age * 0.6));
            r.rx(Slot::RightFront, -0.4 - 0.2 * sin(a.age * 0.6));
            r.rx(Slot::LeftFront, -0.4 - 0.2 * sin(a.age * 0.6));
        }
        if a.scared {
            r.rx(Slot::Head, 2.1707964);
            r.rx(Slot::RightFront, -0.9);
            r.rx(Slot::LeftFront, -0.9);
        }
    } else {
        for s in [
            Slot::RightHind,
            Slot::LeftHind,
            Slot::RightFront,
            Slot::LeftFront,
        ] {
            r.rz(s, 0.0);
        }
    }
    if a.lie_on_back > 0.0 {
        r.rx(Slot::RightHind, -0.6 * sin(a.age * 0.15));
        r.rx(Slot::LeftHind, 0.6 * sin(a.age * 0.15));
        r.rx(Slot::RightFront, 0.3 * sin(a.age * 0.25));
        r.rx(Slot::LeftFront, -0.3 * sin(a.age * 0.25));
        let hx = r.rotation(Slot::Head).x;
        r.rx(Slot::Head, lerp(a.lie_on_back, hx, FRAC_PI_2));
    }
    if a.roll > 0.0 {
        let hx = r.rotation(Slot::Head).x;
        r.rx(Slot::Head, lerp(a.roll, hx, 2.0561945));
        r.rx(Slot::RightHind, -0.5 * sin(a.age * 0.5));
        r.rx(Slot::LeftHind, 0.5 * sin(a.age * 0.5));
        r.rx(Slot::RightFront, 0.5 * sin(a.age * 0.5));
        r.rx(Slot::LeftFront, -0.5 * sin(a.age * 0.5));
    }
}

pub(crate) fn fox(r: &mut R, a: &AnimIn) {
    r.rz(Slot::Head, a.head_roll);
    legs4(r, a, 1.4);
    if a.crouching {
        r.add_rot(Slot::Body, Vec3::new(0.10471976, 0.0, 0.0));
        r.add_pos(Slot::Head, Vec3::new(0.0, a.crouch_amount, 0.0));
        let w = cos(a.age) * 0.05;
        r.ry(Slot::Body, w);
        r.rz(Slot::RightHind, w);
        r.rz(Slot::LeftHind, w);
        r.rz(Slot::RightFront, w / 2.0);
        r.rz(Slot::LeftFront, w / 2.0);
        r.add_pos(Slot::Body, Vec3::new(0.0, a.crouch_amount, 0.0));
        r.add_pos(Slot::Tail, Vec3::new(0.0, 0.0, 0.0));
    } else if a.sleeping {
        for s in [
            Slot::RightHind,
            Slot::LeftHind,
            Slot::RightFront,
            Slot::LeftFront,
        ] {
            r.hide(s);
        }
        r.rz(Slot::Body, -FRAC_PI_2);
        r.add_pos(Slot::Body, Vec3::new(0.0, 5.0, 0.0));
        r.rx(Slot::Tail, -PI * 5.0 / 6.0);
        r.add_pos(Slot::Head, Vec3::new(2.0, 2.99, 0.0));
    } else if a.sitting {
        r.rx(Slot::Body, PI / 6.0);
        r.add_pos(Slot::Body, Vec3::new(0.0, -7.0, 3.0));
        r.rx(Slot::Tail, FRAC_PI_4);
        r.add_pos(Slot::Head, Vec3::new(0.0, -6.5, 2.75));
        r.rx(Slot::RightFront, -PI / 12.0);
        r.rx(Slot::LeftFront, -PI / 12.0);
        for s in [Slot::RightHind, Slot::LeftHind] {
            r.rx(s, -PI * 5.0 / 12.0);
            r.add_pos(s, Vec3::new(0.0, 4.0, -0.25));
        }
        r.add_pos(Slot::Tail, Vec3::new(0.0, 0.0, -1.0));
    }
    if a.sleeping {
        r.rx(Slot::Head, 0.0);
        r.ry(Slot::Head, -PI * 2.0 / 3.0);
        r.rz(Slot::Head, cos(a.age * 0.027) / 22.0);
    } else if !a.crouching && !a.sitting {
        r.rx(Slot::Head, a.head_pitch * DEG);
        r.ry(Slot::Head, a.head_yaw * DEG);
    } else if a.sitting {
        r.rx(Slot::Head, 0.0);
        r.ry(Slot::Head, 0.0);
    }
}

pub(crate) fn witch(r: &mut R, a: &AnimIn) {
    r.rot(
        Slot::Head,
        Some(a.head_pitch * DEG),
        Some(a.head_yaw * DEG),
        None,
    );
    let k = 1.4 * a.walk_speed * 0.5;
    r.rx(Slot::RightLeg, cos(a.walk_pos * 0.6662) * k);
    r.rx(Slot::LeftLeg, cos(a.walk_pos * 0.6662 + PI) * k);
    let speed = 0.01 * (a.entity_id.rem_euclid(10)) as f32;
    r.rot(
        Slot::Nose,
        Some(sin(a.age * speed) * 4.5 * DEG),
        None,
        Some(cos(a.age * speed) * 2.5 * DEG),
    );
    if a.item_held {
        r.set_pos(Slot::Nose, Vec3::new(0.0, 1.0, -1.5));
        r.rx(Slot::Nose, -0.9);
    }
}

pub(crate) fn evoker_fangs(r: &mut R, a: &AnimIn) {
    let bite = a.bite;
    let amount = (bite * 2.0).min(1.0);
    let amount = 1.0 - amount * amount * amount;
    r.rz(Slot::UpperJaw, PI - amount * 0.35 * PI);
    r.rz(Slot::LowerJaw, PI + amount * 0.35 * PI);
    r.add_pos(
        Slot::Base,
        Vec3::new(0.0, -(bite + sin(bite * 2.7)) * 7.2, 0.0),
    );
    let scale = if bite > 0.9 { (1.0 - bite) / 0.1 } else { 1.0 };
    if let Some(p) = r.p(Slot::Root) {
        p.pos.y = 24.0 - 20.0 * scale;
        p.scale = Vec3::splat(scale);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::LayerSet;
    use crate::model::Model;
    use crate::rig::{Binding, Rig, animate};

    fn model(names: &[&str]) -> Model {
        let part = r#"{"pose":[0,0,0,0,0,0,1,1,1],"cubes":[],"children":{}}"#;
        let children: Vec<String> = names.iter().map(|n| format!("\"{n}\":{part}")).collect();
        let json = format!(
            r#"{{"t":{{"tex_w":16,"tex_h":16,"root":{{"pose":[0,0,0,0,0,0,1,1,1],"cubes":[],"children":{{{}}}}}}}}}"#,
            children.join(",")
        );
        Model::from_layer(LayerSet::from_json(&json).unwrap().get("t").unwrap())
    }

    fn run(rig: Rig, names: &[&str], a: AnimIn) -> (Model, Vec<crate::pose::Pose>) {
        let m = model(names);
        let b = Binding::new(&m);
        let mut pose = m.rest_pose();
        animate(rig, &a, &b, &mut pose);
        (m, pose)
    }

    #[test]
    fn squid_tentacles_follow_the_angle() {
        let a = AnimIn {
            tentacle: 0.5,
            ..AnimIn::default()
        };
        let (m, pose) = run(Rig::Squid, &["tentacle0", "tentacle3"], a);
        assert_eq!(pose[m.bone("tentacle3").unwrap() as usize].rot.x, 0.5);
    }

    #[test]
    fn sitting_wolf_tilts_body_and_folds_legs() {
        let a = AnimIn {
            sitting: true,
            tail_angle: 1.0,
            ..AnimIn::default()
        };
        let names = [
            "body",
            "head",
            "real_head",
            "upper_body",
            "tail",
            "right_hind_leg",
            "right_front_leg",
        ];
        let (m, pose) = run(Rig::Wolf, &names, a);
        let rx = |n: &str| pose[m.bone(n).unwrap() as usize].rot.x;
        assert!((rx("body") - FRAC_PI_4).abs() < 1e-5);
        assert!((rx("right_hind_leg") - PI * 1.5).abs() < 1e-5);
        assert_eq!(rx("tail"), 1.0);
    }

    #[test]
    fn blaze_rods_orbit_at_radius_nine() {
        let names = ["head", "part0", "part1", "part2", "part3"];
        let (m, pose) = run(Rig::Blaze, &names, AnimIn::default());
        let p = pose[m.bone("part0").unwrap() as usize].pos;
        assert!((p.x - 9.0).abs() < 1e-4 && p.z.abs() < 1e-4);
    }

    #[test]
    fn magma_cube_stretches_with_squish() {
        let a = AnimIn {
            squish: 1.0,
            ..AnimIn::default()
        };
        let (m, pose) = run(Rig::Slime, &["cube0", "cube3"], a);
        assert!((pose[m.bone("cube0").unwrap() as usize].pos.y + 6.8).abs() < 1e-4);
    }

    #[test]
    fn horse_rearing_raises_the_front() {
        let a = AnimIn {
            stand: 1.0,
            ..AnimIn::default()
        };
        let names = [
            "body",
            "head_parts",
            "tail",
            "left_front_leg",
            "right_front_leg",
            "left_hind_leg",
            "right_hind_leg",
        ];
        let (m, pose) = run(Rig::Equine, &names, a);
        assert!((pose[m.bone("body").unwrap() as usize].rot.x + FRAC_PI_4).abs() < 1e-5);
    }

    #[test]
    fn evoker_fangs_open_rise_and_shrink_away() {
        let names = ["base", "upper_jaw", "lower_jaw"];
        let bite = |bite: f32| {
            let (m, pose) = run(
                Rig::Fangs,
                &names,
                AnimIn {
                    bite,
                    ..AnimIn::default()
                },
            );
            let get = |n: &str| pose[m.bone(n).unwrap() as usize];
            (get("upper_jaw").rot.z, get("base").pos.y, pose[0].scale.x)
        };
        let (jaw, base, scale) = bite(0.05);
        assert!((jaw - PI * 0.65).abs() < 0.01 && base < 0.0 && scale == 1.0);
        let (open, _, _) = bite(0.5);
        assert!((open - PI).abs() < 1e-5);
        assert!((bite(0.95).2 - 0.5).abs() < 1e-4);
    }

    #[test]
    fn stunned_ravager_shakes_its_neck() {
        let a = AnimIn {
            stunned: 30.0,
            ..AnimIn::default()
        };
        let (m, pose) = run(Rig::Ravager, &["neck", "mouth"], a);
        let neck = pose[m.bone("neck").unwrap() as usize];
        assert!((neck.rot.x - 0.21991149).abs() < 1e-6);
        assert!((neck.pos.x - (7.5f32).sin() * 3.0).abs() < 1e-4);
    }

    #[test]
    fn gliding_damps_the_limb_swing_by_speed_value() {
        use crate::rig::HumanoidStyle;
        let mut data = rustc_hash::FxHashMap::default();
        data.insert(
            0u8,
            mcr_protocol::types::entity_data::EntityDataValue::Byte(0x80u8 as i8),
        );
        let mut v = crate::view::EntityView::named("minecraft:player", &data);
        v.limb_amount = 1.0;
        v.glide.speed = 125.0;
        let a = crate::catalog::base_anim(&v);
        assert!(a.fall_flying && a.speed_value == 125.0);
        let names = ["head", "right_leg", "left_leg", "right_arm", "left_arm"];
        let (m, pose) = run(Rig::Humanoid(HumanoidStyle::Plain), &names, a);
        let leg = pose[m.bone("right_leg").unwrap() as usize];
        assert!((leg.rot.x - 1.4 / 125.0).abs() < 1e-6, "{}", leg.rot.x);
        let head = pose[m.bone("head").unwrap() as usize];
        assert!((head.rot.x + std::f32::consts::FRAC_PI_4).abs() < 1e-6);
    }

    #[test]
    fn triangle_wave_peaks_at_the_quarter_period() {
        assert!((triangle_wave(0.0, 10.0) - 1.0).abs() < 1e-6);
        assert!((triangle_wave(5.0, 10.0) + 1.0).abs() < 1e-6);
    }
}
