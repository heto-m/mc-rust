use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};

use glam::Vec3;

use crate::model::{BoneId, Model};
use crate::pose::Pose;

pub(crate) const DEG: f32 = PI / 180.0;
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
    InnerRoot = "root",
    BodyBack = "body_back", UpperMouth = "upper_mouth", LowerMouth = "lower_mouth", Eye = "eye",
    Spike0 = "spike0", Spike1 = "spike1", Spike2 = "spike2", Spike3 = "spike3", Spike4 = "spike4",
    Spike5 = "spike5", Spike6 = "spike6", Spike7 = "spike7", Spike8 = "spike8", Spike9 = "spike9",
    Spike10 = "spike10", Spike11 = "spike11",
    Tail0 = "tail0", Tail3 = "tail3", Tail4 = "tail4", Tail5 = "tail5", Tail6 = "tail6",
    Tail7 = "tail7", Tail8 = "tail8", Tail9 = "tail9", Tail10 = "tail10", Tail11 = "tail11",
    Neck = "neck", Neck0 = "neck0", Neck1 = "neck1", Neck2 = "neck2", Neck3 = "neck3", Neck4 = "neck4",
    Jaw = "jaw",
    LeftWingBase = "left_wing_base", RightWingBase = "right_wing_base",
    LeftWingTip = "left_wing_tip", RightWingTip = "right_wing_tip",
    TailBase = "tail_base", TailTip = "tail_tip",
    CenterHead = "center_head", LeftHead = "left_head", RightHead = "right_head", Ribcage = "ribcage",
    Part0 = "part0", Part1 = "part1", Part2 = "part2", Part3 = "part3", Part4 = "part4",
    Part5 = "part5", Part6 = "part6", Part7 = "part7", Part8 = "part8", Part9 = "part9",
    Part10 = "part10", Part11 = "part11",
    Cube0 = "cube0", Cube1 = "cube1", Cube2 = "cube2", Cube3 = "cube3", Cube4 = "cube4",
    Cube5 = "cube5", Cube6 = "cube6", Cube7 = "cube7",
    Lid = "lid",
    LeftFrontLegTip = "left_front_leg_tip", LeftFrontFoot = "left_front_foot",
    LeftHindLegTip = "left_hind_leg_tip", LeftHindFoot = "left_hind_foot",
    RightFrontLegTip = "right_front_leg_tip", RightFrontFoot = "right_front_foot",
    RightHindLegTip = "right_hind_leg_tip", RightHindFoot = "right_hind_foot",
    LeftBottomBristle = "left_bottom_bristle", LeftMiddleBristle = "left_middle_bristle",
    LeftTopBristle = "left_top_bristle", RightBottomBristle = "right_bottom_bristle",
    RightMiddleBristle = "right_middle_bristle", RightTopBristle = "right_top_bristle",
    Bristle0 = "bristle0", Bristle1 = "bristle1", Bristle2 = "bristle2",
    LeftBlueFin = "left_blue_fin", RightBlueFin = "right_blue_fin",
    Segment0 = "segment0", Segment1 = "segment1", Segment2 = "segment2", Segment3 = "segment3",
    Segment4 = "segment4", Segment5 = "segment5",
    BackFin = "back_fin", Stinger = "stinger", FrontLegs = "front_legs", MiddleLegs = "middle_legs",
    BackLegs = "back_legs",
    LeftFrontBristle = "left_front_bristle",
    EggBelly = "egg_belly",
    LeftHorn = "left_horn", RightHorn = "right_horn",
    Feet = "feet",
    Layer0 = "layer0", Layer1 = "layer1", Layer2 = "layer2", Segment6 = "segment6",
    OuterGlass = "outer_glass", InnerGlass = "inner_glass", Base = "base",
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
        let nested = model
            .bones
            .iter()
            .position(|b| b.name == "root" && b.parent == Some(0));
        if let Some(i) = nested {
            ids[Slot::InnerRoot as usize] = i as BoneId;
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
    Spear,
}

impl ArmPose {
    pub(crate) fn two_handed(self) -> bool {
        matches!(
            self,
            ArmPose::BowAndArrow | ArmPose::CrossbowCharge | ArmPose::CrossbowHold
        )
    }

    pub(crate) fn affects_offhand(self) -> bool {
        matches!(
            self,
            ArmPose::BowAndArrow
                | ArmPose::ThrowTrident
                | ArmPose::CrossbowCharge
                | ArmPose::CrossbowHold
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IllagerPose {
    #[default]
    Crossed,
    Neutral,
    Attacking,
    Spellcasting,
    BowAndArrow,
    CrossbowHold,
    CrossbowCharge,
    Celebrating,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PiglinPose {
    #[default]
    Default,
    Dancing,
    MeleeAttack,
    CrossbowHold,
    CrossbowCharge,
    Admiring,
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
    pub main_hand_empty: bool,
    pub holding_bow: bool,
    pub illager: IllagerPose,
    pub piglin: PiglinPose,
    pub charge_max: f32,
    pub head_roll: f32,
    pub in_water: bool,
    pub squish: f32,
    pub tail_angle: f32,
    pub shake: f32,
    pub stand: f32,
    pub eat: f32,
    pub feed: f32,
    pub peek: f32,
    pub charging: bool,
    pub holding: f32,
    pub flap: f32,
    pub tentacle: f32,
    pub puff: u8,
    pub carrying: bool,
    pub creepy: bool,
    pub sit_amount: f32,
    pub roll: f32,
    pub lie_on_back: f32,
    pub sneeze: f32,
    pub scared: bool,
    pub crouch_amount: f32,
    pub sleeping: bool,
    pub entity_id: i32,
    pub has_chest: bool,
    pub on_land: bool,
    pub rearing: bool,
    pub has_stinger: bool,
    pub item_held: bool,
    pub bristle: f32,
    pub attack_ticks: f32,
    pub offer_ticks: f32,
    pub donkey: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HumanoidStyle {
    Plain,
    Zombie,
    Skeleton,
    Illager,
    Piglin,
    ZombifiedPiglin,
    Drowned,
    ArmorStand,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Rig {
    Static,
    Look,
    Humanoid(HumanoidStyle),
    Illager,
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
    Pufferfish,
    Guardian,
    Dragon,
    Wither,
    Ravager,
    Shulker,
    Dolphin,
    Panda,
    Witch,
    Silverfish,
    Crystal,
}

pub(crate) struct R<'a> {
    pub(crate) b: &'a Binding,
    pub(crate) pose: &'a mut [Pose],
}

impl R<'_> {
    pub(crate) fn p(&mut self, s: Slot) -> Option<&mut Pose> {
        self.b.get(s).map(|i| &mut self.pose[i])
    }

    pub(crate) fn rot(&mut self, s: Slot, x: Option<f32>, y: Option<f32>, z: Option<f32>) {
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

    pub(crate) fn rx(&mut self, s: Slot, v: f32) {
        self.rot(s, Some(v), None, None);
    }

    pub(crate) fn ry(&mut self, s: Slot, v: f32) {
        self.rot(s, None, Some(v), None);
    }

    pub(crate) fn rz(&mut self, s: Slot, v: f32) {
        self.rot(s, None, None, Some(v));
    }

    pub(crate) fn add_rot(&mut self, s: Slot, d: Vec3) {
        if let Some(p) = self.p(s) {
            p.rot += d;
        }
    }

    pub(crate) fn set_pos(&mut self, s: Slot, v: Vec3) {
        if let Some(p) = self.p(s) {
            p.pos = v;
        }
    }

    pub(crate) fn pos(&self, s: Slot) -> Vec3 {
        self.b.get(s).map_or(Vec3::ZERO, |i| self.pose[i].pos)
    }

    pub(crate) fn rotate_by(&mut self, s: Slot, q: glam::Quat) {
        if let Some(p) = self.p(s) {
            let old = glam::Quat::from_euler(glam::EulerRot::ZYX, p.rot.z, p.rot.y, p.rot.x);
            let (z, y, x) = (old * q).to_euler(glam::EulerRot::ZYX);
            p.rot = Vec3::new(x, y, z);
        }
    }

    pub(crate) fn hide(&mut self, s: Slot) {
        if let Some(p) = self.p(s) {
            p.scale = Vec3::ZERO;
        }
    }

    pub(crate) fn add_pos(&mut self, s: Slot, d: Vec3) {
        if let Some(p) = self.p(s) {
            p.pos += d;
        }
    }

    pub(crate) fn rotation(&self, s: Slot) -> Vec3 {
        self.b.get(s).map_or(Vec3::ZERO, |i| self.pose[i].rot)
    }

    pub(crate) fn look(&mut self, a: &AnimIn) {
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

pub(crate) fn cos(x: f32) -> f32 {
    x.cos()
}

pub(crate) fn lerp(t: f32, a: f32, b: f32) -> f32 {
    a + t * (b - a)
}

/// Applies the family rig on top of `pose` (the model's rest pose).
pub fn animate(rig: Rig, a: &AnimIn, b: &Binding, pose: &mut [Pose]) {
    let mut r = R { b, pose };
    match rig {
        Rig::Static => {}
        Rig::Look => r.look(a),
        Rig::Humanoid(style) => crate::rig_humanoid::humanoid(&mut r, a, style),
        Rig::Illager => crate::rig_humanoid::illager(&mut r, a),
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
        Rig::Wolf => crate::rig_mobs::wolf(&mut r, a),
        Rig::Slime => crate::rig_mobs::slime(&mut r, a),
        Rig::Fish => crate::rig_mobs::fish(&mut r, a),
        Rig::Pufferfish => crate::rig_mobs::pufferfish(&mut r, a),
        Rig::Squid => crate::rig_mobs::squid(&mut r, a),
        Rig::Blaze => crate::rig_mobs::blaze(&mut r, a),
        Rig::Ghast => crate::rig_mobs::ghast(&mut r, a),
        Rig::Enderman => crate::rig_mobs::enderman(&mut r, a),
        Rig::SnowGolem => crate::rig_mobs::snow_golem(&mut r, a),
        Rig::IronGolem => crate::rig_mobs::iron_golem(&mut r, a),
        Rig::Guardian => crate::rig_mobs::guardian(&mut r, a),
        Rig::Phantom => crate::rig_mobs::phantom(&mut r, a),
        Rig::Dragon => crate::rig_mobs::dragon(&mut r, a),
        Rig::Wither => crate::rig_mobs::wither(&mut r, a),
        Rig::Equine => crate::rig_mobs::equine(&mut r, a),
        Rig::Llama => crate::rig_mobs::llama(&mut r, a),
        Rig::Strider => crate::rig_mobs::strider(&mut r, a),
        Rig::Hoglin => crate::rig_mobs::hoglin(&mut r, a),
        Rig::Ravager => crate::rig_mobs::ravager(&mut r, a),
        Rig::Shulker => crate::rig_mobs::shulker(&mut r, a),
        Rig::Vex => crate::rig_mobs::vex(&mut r, a),
        Rig::Allay => crate::rig_mobs::allay(&mut r, a),
        Rig::Parrot => crate::rig_mobs::parrot(&mut r, a),
        Rig::Endermite => crate::rig_mobs::endermite(&mut r, a, false),
        Rig::Silverfish => crate::rig_mobs::endermite(&mut r, a, true),
        Rig::Dolphin => crate::rig_mobs::dolphin(&mut r, a),
        Rig::Bee => crate::rig_mobs::bee(&mut r, a),
        Rig::Turtle => crate::rig_mobs::turtle(&mut r, a),
        Rig::Polar => crate::rig_mobs::polar_bear(&mut r, a),
        Rig::Panda => crate::rig_mobs::panda(&mut r, a),
        Rig::Fox => crate::rig_mobs::fox(&mut r, a),
        Rig::Witch => crate::rig_mobs::witch(&mut r, a),
        Rig::Crystal => crate::rig_mobs::crystal(&mut r, a),
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
