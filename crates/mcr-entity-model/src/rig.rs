use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};

use glam::Vec3;

use crate::model::{BoneId, Model};
use crate::pose::Pose;

const DEG: f32 = PI / 180.0;
const ABSENT: BoneId = BoneId::MAX;

macro_rules! slots {
    ($($v:ident = $n:literal),* $(,)?) => {
        /// Semantic bones the hand-written rigs address; resolved to bone ids once per model.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #[repr(usize)]
        pub enum Slot { $($v),* }

        impl Slot {
            pub const ALL: &'static [(Slot, &'static str)] = &[$((Slot::$v, $n)),*];
        }
    };
}

slots! {
    Head = "head", Hat = "hat", Body = "body", RightArm = "right_arm", LeftArm = "left_arm",
    RightLeg = "right_leg", LeftLeg = "left_leg",
    RightHind = "right_hind_leg", LeftHind = "left_hind_leg", RightFront = "right_front_leg",
    LeftFront = "left_front_leg",
    RightWing = "right_wing", LeftWing = "left_wing", Tail = "tail", Tail1 = "tail1", Tail2 = "tail2",
    Arms = "arms", Nose = "nose", Jacket = "jacket", Root = "root",
    Beak = "beak",
    RightMidHind = "right_middle_hind_leg", LeftMidHind = "left_middle_hind_leg",
    RightMidFront = "right_middle_front_leg", LeftMidFront = "left_middle_front_leg",
    UpperBody = "upper_body", RealHead = "real_head", RealTail = "real_tail",
    Mane = "mane", HeadParts = "head_parts", LeftEar = "left_ear", RightEar = "right_ear",
    Cube = "cube", Mouth = "mouth", LeftEye = "left_eye", RightEye = "right_eye",
    Bone = "bone", Body0 = "body0", Body1 = "body1",
    LowerBody = "lower_body",
    RightFin = "right_fin", LeftFin = "left_fin", TailFin = "tail_fin", TopFin = "top_fin",
    Backlegs = "backlegs", Frontlegs = "frontlegs", LeftHaunch = "left_haunch", RightHaunch = "right_haunch",
    LeftSleeve = "left_sleeve", RightSleeve = "right_sleeve", LeftPants = "left_pants",
    RightPants = "right_pants", BasePlate = "base_plate", LeftChest = "left_chest",
    RightChest = "right_chest", LeftBodyStick = "left_body_stick", RightBodyStick = "right_body_stick",
    ShoulderStick = "shoulder_stick", Cape = "cape", Wool = "wool",
    Tentacle0 = "tentacle0", Tentacle1 = "tentacle1", Tentacle2 = "tentacle2", Tentacle3 = "tentacle3",
    Tentacle4 = "tentacle4", Tentacle5 = "tentacle5", Tentacle6 = "tentacle6", Tentacle7 = "tentacle7",
    Tentacle8 = "tentacle8",
}

const SLOT_COUNT: usize = Slot::ALL.len();

/// A model's bones resolved for the rigs.
#[derive(Debug, Clone)]
pub struct Binding {
    ids: [BoneId; SLOT_COUNT],
}

impl Binding {
    pub fn new(model: &Model) -> Self {
        let mut ids = [ABSENT; SLOT_COUNT];
        for (slot, name) in Slot::ALL {
            if let Some(id) = model.bone(name) {
                ids[*slot as usize] = id;
            }
        }
        Binding { ids }
    }

    pub fn get(&self, slot: Slot) -> Option<usize> {
        let id = self.ids[slot as usize];
        (id != ABSENT).then_some(id as usize)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ArmPose {
    #[default]
    Empty,
    Item,
    Block,
    BowAndArrow,
    ThrowTrident,
    CrossbowCharge,
    CrossbowHold,
    Spyglass,
    TootHorn,
    Brush,
}

impl ArmPose {
    fn two_handed(self) -> bool {
        matches!(
            self,
            ArmPose::BowAndArrow
                | ArmPose::ThrowTrident
                | ArmPose::CrossbowCharge
                | ArmPose::CrossbowHold
        )
    }

    fn affects_offhand(self) -> bool {
        matches!(
            self,
            ArmPose::BowAndArrow | ArmPose::CrossbowCharge | ArmPose::CrossbowHold
        )
    }
}

/// Inputs of vanilla's `setupAnim` that the rigs consume; angles in degrees.
#[derive(Debug, Clone, Copy, Default)]
pub struct AnimIn {
    pub walk_pos: f32,
    pub walk_speed: f32,
    pub age: f32,
    pub head_yaw: f32,
    pub head_pitch: f32,
    pub baby: bool,
    pub on_ground: bool,
    pub attack: f32,
    pub swing_left: bool,
    pub crouching: bool,
    pub sprinting: bool,
    pub sitting: bool,
    pub passenger: bool,
    pub aggressive: bool,
    pub swim_amount: f32,
    pub fall_flying: bool,
    pub right_arm: ArmPose,
    pub left_arm: ArmPose,
    pub main_right: bool,
    pub use_ticks: f32,
    pub unhappy: bool,
    pub lie_down: f32,
    pub extra: [f32; 4],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HumanoidStyle {
    Plain,
    Zombie,
    Skeleton,
    Illager,
    Piglin,
    ArmorStand,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Rig {
    Static,
    Look,
    Humanoid(HumanoidStyle),
    Quadruped,
    Creeper,
    Chicken,
    Spider,
    Villager,
    Feline,
    Wolf,
    Slime,
    Rabbit,
    Equine,
    IronGolem,
    Enderman,
    SnowGolem,
    Ghast,
    Squid,
    Bat,
    Bee,
    Blaze,
    Parrot,
    Phantom,
    Fish,
    Endermite,
    Turtle,
    Llama,
    Fox,
    Polar,
    Hoglin,
    Strider,
    Allay,
    Vex,
}

struct R<'a> {
    b: &'a Binding,
    pose: &'a mut [Pose],
}

impl R<'_> {
    fn p(&mut self, s: Slot) -> Option<&mut Pose> {
        self.b.get(s).map(|i| &mut self.pose[i])
    }

    fn rot(&mut self, s: Slot, x: Option<f32>, y: Option<f32>, z: Option<f32>) {
        if let Some(p) = self.p(s) {
            if let Some(x) = x {
                p.rot.x = x;
            }
            if let Some(y) = y {
                p.rot.y = y;
            }
            if let Some(z) = z {
                p.rot.z = z;
            }
        }
    }

    fn rx(&mut self, s: Slot, v: f32) {
        self.rot(s, Some(v), None, None);
    }

    fn ry(&mut self, s: Slot, v: f32) {
        self.rot(s, None, Some(v), None);
    }

    fn rz(&mut self, s: Slot, v: f32) {
        self.rot(s, None, None, Some(v));
    }

    fn add_rot(&mut self, s: Slot, d: Vec3) {
        if let Some(p) = self.p(s) {
            p.rot += d;
        }
    }

    fn add_pos(&mut self, s: Slot, d: Vec3) {
        if let Some(p) = self.p(s) {
            p.pos += d;
        }
    }

    fn rotation(&self, s: Slot) -> Vec3 {
        self.b.get(s).map_or(Vec3::ZERO, |i| self.pose[i].rot)
    }

    fn look(&mut self, a: &AnimIn) {
        if self.b.get(Slot::Head).is_some() {
            self.rot(
                Slot::Head,
                Some(a.head_pitch * DEG),
                Some(a.head_yaw * DEG),
                None,
            );
        } else if let Some(p) = self.p(Slot::HeadParts) {
            p.rot.x += a.head_pitch * DEG;
            p.rot.y = a.head_yaw * DEG;
        }
    }
}

fn cos(x: f32) -> f32 {
    x.cos()
}

fn lerp(t: f32, a: f32, b: f32) -> f32 {
    a + t * (b - a)
}

/// Applies the family rig on top of `pose` (the model's rest pose).
pub fn animate(rig: Rig, a: &AnimIn, b: &Binding, pose: &mut [Pose]) {
    let mut r = R { b, pose };
    match rig {
        Rig::Static => {}
        Rig::Look => r.look(a),
        Rig::Humanoid(style) => humanoid(&mut r, a, style),
        Rig::Quadruped => {
            r.look(a);
            legs4(&mut r, a, 1.0);
        }
        Rig::Creeper => {
            r.look(a);
            legs4(&mut r, a, 1.0);
        }
        Rig::Chicken => {
            r.look(a);
            let swing = a.walk_pos * 0.6662;
            r.rx(Slot::RightLeg, cos(swing) * 1.4 * a.walk_speed);
            r.rx(Slot::LeftLeg, cos(swing + PI) * 1.4 * a.walk_speed);
            if !a.on_ground {
                let flap = ((a.age * 0.6).sin() + 1.0) * 1.0;
                r.rz(Slot::RightWing, flap);
                r.rz(Slot::LeftWing, -flap);
            }
        }
        Rig::Spider => spider(&mut r, a),
        Rig::Villager => {
            r.look(a);
            if a.unhappy {
                r.rz(Slot::Head, 0.3 * (0.45 * a.age).sin());
                r.rx(Slot::Head, 0.4);
            }
            r.rx(
                Slot::RightLeg,
                cos(a.walk_pos * 0.6662) * 1.4 * a.walk_speed * 0.5,
            );
            r.rx(
                Slot::LeftLeg,
                cos(a.walk_pos * 0.6662 + PI) * 1.4 * a.walk_speed * 0.5,
            );
            r.ry(Slot::RightLeg, 0.0);
            r.ry(Slot::LeftLeg, 0.0);
        }
        Rig::Feline => feline(&mut r, a),
        Rig::Wolf => wolf(&mut r, a),
        Rig::Slime => {}
        _ => {
            r.look(a);
        }
    }
}

fn legs4(r: &mut R, a: &AnimIn, amount: f32) {
    let swing = a.walk_pos * 0.6662;
    let k = 1.4 * a.walk_speed * amount;
    r.rx(Slot::RightHind, cos(swing) * k);
    r.rx(Slot::LeftHind, cos(swing + PI) * k);
    r.rx(Slot::RightFront, cos(swing + PI) * k);
    r.rx(Slot::LeftFront, cos(swing) * k);
}

fn spider(r: &mut R, a: &AnimIn) {
    r.look(a);
    let pos = a.walk_pos * 0.6662;
    let sp = a.walk_speed;
    let swing = |off: f32| -(cos(pos * 2.0 + off) * 0.4) * sp;
    let step = |off: f32| (pos + off).sin().abs() * 0.4 * sp;
    let hind = (swing(0.0), step(0.0));
    let mid_hind = (swing(PI), step(PI));
    let mid_front = (swing(FRAC_PI_2), step(FRAC_PI_2));
    let front = (swing(PI * 1.5), step(PI * 1.5));
    for (right, left, (sw, st)) in [
        (Slot::RightHind, Slot::LeftHind, hind),
        (Slot::RightMidHind, Slot::LeftMidHind, mid_hind),
        (Slot::RightMidFront, Slot::LeftMidFront, mid_front),
        (Slot::RightFront, Slot::LeftFront, front),
    ] {
        r.add_rot(right, Vec3::new(0.0, sw, st));
        r.add_rot(left, Vec3::new(0.0, -sw, -st));
    }
}

fn feline(r: &mut R, a: &AnimIn) {
    let (pos, sp) = (a.walk_pos, a.walk_speed);
    let age = if a.baby { 0.5 } else { 1.0 };
    if a.crouching {
        r.add_pos(Slot::Body, Vec3::new(0.0, 1.0 * age, 0.0));
        r.add_pos(Slot::Head, Vec3::new(0.0, 2.0 * age, 0.0));
        r.add_pos(Slot::Tail1, Vec3::new(0.0, 1.0 * age, 0.0));
        r.add_pos(Slot::Tail2, Vec3::new(0.0, -4.0 * age, 2.0 * age));
        r.rx(Slot::Tail1, FRAC_PI_2);
        r.rx(Slot::Tail2, FRAC_PI_2);
    } else if a.sprinting {
        if let (Some(t1), Some(t2)) = (r.b.get(Slot::Tail1), r.b.get(Slot::Tail2)) {
            r.pose[t2].pos.y = r.pose[t1].pos.y;
        }
        r.add_pos(Slot::Tail2, Vec3::new(0.0, 0.0, 2.0 * age));
        r.rx(Slot::Tail1, FRAC_PI_2);
        r.rx(Slot::Tail2, FRAC_PI_2);
    }
    r.look(a);
    if !a.sitting {
        r.rx(Slot::Body, FRAC_PI_2);
        if a.sprinting {
            r.rx(Slot::LeftHind, cos(pos * 0.6662) * sp);
            r.rx(Slot::RightHind, cos(pos * 0.6662 + 0.3) * sp);
            r.rx(Slot::LeftFront, cos(pos * 0.6662 + PI + 0.3) * sp);
            r.rx(Slot::RightFront, cos(pos * 0.6662 + PI) * sp);
            r.rx(Slot::Tail2, 1.7278761 + PI / 10.0 * cos(pos) * sp);
        } else {
            r.rx(Slot::LeftHind, cos(pos * 0.6662) * sp);
            r.rx(Slot::RightHind, cos(pos * 0.6662 + PI) * sp);
            r.rx(Slot::LeftFront, cos(pos * 0.6662 + PI) * sp);
            r.rx(Slot::RightFront, cos(pos * 0.6662) * sp);
            let amp = if a.crouching { 0.47123894 } else { FRAC_PI_4 };
            r.rx(Slot::Tail2, 1.7278761 + amp * cos(pos) * sp);
        }
    } else {
        r.rx(Slot::Body, FRAC_PI_4);
        r.add_pos(Slot::Body, Vec3::new(0.0, -4.0 * age, 5.0 * age));
        r.add_pos(Slot::Head, Vec3::new(0.0, -3.3 * age, 1.0 * age));
        r.add_pos(Slot::Tail1, Vec3::new(0.0, 8.0 * age, -2.0 * age));
        r.add_pos(Slot::Tail2, Vec3::new(0.0, 2.0 * age, -0.8 * age));
        r.rx(Slot::Tail1, 1.7278761);
        r.rx(Slot::Tail2, 2.670354);
        for s in [Slot::LeftFront, Slot::RightFront] {
            r.rx(s, -PI / 20.0);
            r.add_pos(s, Vec3::new(0.0, 2.0 * age, -2.0 * age));
        }
        for s in [Slot::LeftHind, Slot::RightHind] {
            r.rx(s, -FRAC_PI_2);
            r.add_pos(s, Vec3::new(0.0, 3.0 * age, -4.0 * age));
        }
    }
    if a.lie_down > 0.0 {
        let t = a.lie_down;
        let hz = r.rotation(Slot::Head).z;
        let hy = r.rotation(Slot::Head).y;
        r.rz(Slot::Head, lerp(t, hz, -1.2707963));
        r.ry(Slot::Head, lerp(t, hy, 1.2707963));
        r.rx(Slot::LeftFront, -1.2707963);
        r.rx(Slot::RightFront, -0.47079635);
        r.rz(Slot::RightFront, -0.2);
        r.add_pos(Slot::RightFront, Vec3::new(age, 0.0, 0.0));
        r.rx(Slot::LeftHind, -0.4);
        r.rx(Slot::RightHind, 0.5);
        r.rz(Slot::RightHind, -0.5);
        r.add_pos(Slot::RightHind, Vec3::new(0.8 * age, 2.0 * age, 0.0));
    }
}

fn wolf(r: &mut R, a: &AnimIn) {
    let (pos, sp) = (a.walk_pos, a.walk_speed);
    r.look(a);
    if a.sitting {
        r.add_pos(Slot::UpperBody, Vec3::new(0.0, 0.0, 0.0));
    }
    if !a.sitting {
        r.rx(Slot::RightHind, cos(pos * 0.6662) * 1.4 * sp);
        r.rx(Slot::LeftHind, cos(pos * 0.6662 + PI) * 1.4 * sp);
        r.rx(Slot::RightFront, cos(pos * 0.6662 + PI) * 1.4 * sp);
        r.rx(Slot::LeftFront, cos(pos * 0.6662) * 1.4 * sp);
    }
}

fn humanoid(r: &mut R, a: &AnimIn, style: HumanoidStyle) {
    let head_x = a.head_pitch * DEG;
    let head_y = a.head_yaw * DEG;
    r.look(a);
    if a.fall_flying {
        r.rx(Slot::Head, -FRAC_PI_4);
    }
    let (pos, sp) = (a.walk_pos, a.walk_speed);
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

    attack(r, a, head_x);
    match style {
        HumanoidStyle::Zombie => zombie_arms(r, a),
        HumanoidStyle::Skeleton => {
            if a.aggressive && a.right_arm != ArmPose::BowAndArrow {
                let s2 = (a.attack * PI).sin();
                let s1 = ((1.0 - (1.0 - a.attack) * (1.0 - a.attack)) * PI).sin();
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
        HumanoidStyle::Illager => {
            if a.aggressive {
                if let Some(p) = r.p(Slot::Arms) {
                    p.scale = Vec3::ZERO;
                }
            } else {
                for s in [Slot::LeftArm, Slot::RightArm] {
                    if let Some(p) = r.p(s) {
                        p.scale = Vec3::ZERO;
                    }
                }
            }
        }
        _ => {}
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
        swim(r, a);
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

fn zombie_arms(r: &mut R, a: &AnimIn) {
    let raise = !a.baby || a.right_arm == ArmPose::Empty && a.left_arm == ArmPose::Empty;
    let drop = if raise {
        -PI / if a.aggressive { 1.5 } else { 2.25 }
    } else {
        0.0
    };
    let ay = (if raise { 1.0 } else { -1.0 }) * (a.attack * PI).sin();
    let ax = ((1.0 - (1.0 - a.attack) * (1.0 - a.attack)) * PI).sin();
    let x = drop + ay * 1.2 - ax * 0.4;
    let y = 0.1 - ay * 0.6;
    r.rot(
        Slot::RightArm,
        Some(x),
        Some(if raise { -y } else { y }),
        Some(0.0),
    );
    r.rot(
        Slot::LeftArm,
        Some(x),
        Some(if raise { y } else { -y }),
        Some(0.0),
    );
}

fn pose_arm(r: &mut R, a: &AnimIn, right: bool, head_x: f32, head_y: f32) {
    let (arm, other) = if right {
        (Slot::RightArm, Slot::LeftArm)
    } else {
        (Slot::LeftArm, Slot::RightArm)
    };
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
        ArmPose::CrossbowCharge => {
            let (arm_main, arm_off) = (arm, other);
            let progress = (a.use_ticks / 25.0).clamp(0.0, 1.0);
            let sign = if right { 1.0 } else { -1.0 };
            r.ry(arm_main, 0.4 * -sign + head_y * 0.0);
            r.ry(arm_off, 0.6 * sign);
            r.rx(arm_main, -FRAC_PI_2 + head_x + 0.1);
            r.rx(arm_off, 1.5 + head_x - (1.5 - 0.6) * progress);
            let _ = arm_off;
        }
        ArmPose::CrossbowHold => {
            let sign = if right { 1.0 } else { -1.0 };
            r.ry(arm, -0.3 * sign + head_y);
            r.ry(other, 0.6 * sign + head_y);
            r.rx(arm, -FRAC_PI_2 + head_x + 0.1);
            r.rx(other, -1.5 + head_x);
        }
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
                Some(lerp(sa, cur.x, x)),
                Some(lerp(sa, cur.y, PI)),
                Some(lerp(sa, cur.z, z)),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::LayerSet;

    fn biped() -> Model {
        let part = r#"{"pose":[0,0,0,0,0,0,1,1,1],"cubes":[],"children":{}}"#;
        let json = format!(
            r#"{{"t":{{"tex_w":16,"tex_h":16,"root":{{"pose":[0,0,0,0,0,0,1,1,1],"cubes":[],"children":{{
            "head":{part},"body":{part},"right_arm":{part},"left_arm":{part},"right_leg":{part},"left_leg":{part}}}}}}}}}"#
        );
        Model::from_layer(LayerSet::from_json(&json).unwrap().get("t").unwrap())
    }

    #[test]
    fn crouching_leans_body_and_drops_head() {
        let m = biped();
        let b = Binding::new(&m);
        let mut pose = m.rest_pose();
        let a = AnimIn {
            crouching: true,
            ..AnimIn::default()
        };
        animate(Rig::Humanoid(HumanoidStyle::Plain), &a, &b, &mut pose);
        let body = pose[m.bone("body").unwrap() as usize];
        let head = pose[m.bone("head").unwrap() as usize];
        assert_eq!(body.rot.x, 0.5);
        assert!((head.pos.y - 4.2).abs() < 1e-5);
    }

    #[test]
    fn walking_swings_arms_against_legs() {
        let m = biped();
        let b = Binding::new(&m);
        let mut pose = m.rest_pose();
        let a = AnimIn {
            walk_speed: 1.0,
            ..AnimIn::default()
        };
        animate(Rig::Humanoid(HumanoidStyle::Plain), &a, &b, &mut pose);
        let rx = |n: &str| pose[m.bone(n).unwrap() as usize].rot.x;
        assert!(rx("right_leg") > 1.0 && rx("left_leg") < -1.0);
        assert!(rx("right_arm") < -0.5 && rx("left_arm") > 0.5);
    }

    #[test]
    fn zombie_arms_point_forward() {
        let m = biped();
        let b = Binding::new(&m);
        let mut pose = m.rest_pose();
        animate(
            Rig::Humanoid(HumanoidStyle::Zombie),
            &AnimIn::default(),
            &b,
            &mut pose,
        );
        let arm = pose[m.bone("right_arm").unwrap() as usize].rot.x;
        assert!((arm + PI / 2.25).abs() < 0.1, "{arm}");
    }
}
