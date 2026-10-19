use glam::{Mat4, Vec3};
use mcr_protocol::types::entity_data::{EntityDataValue, Pose};
use uuid::Uuid;

use crate::catalog::{BuildFn, PlanCx, hitbox};
use crate::equipment::{add_layers, has_layer, humanoid_armor};
use crate::hands::{
    HandFamily, arm_items, hand_attach, is_item, is_spear, item_name, player_poses, set_pose,
};
use crate::idx;
use crate::mobs::{short, variant_name};
use crate::plan::{Attach, AttachKind, ItemContext, ItemSource, Part, Plan};
use crate::rig::{ArmPose, HumanoidStyle, IllagerPose, PiglinPose, Rig, Slot};
use crate::view::{EntityView, slot};

struct Hum {
    layer: &'static str,
    baby_layer: &'static str,
    tex: &'static str,
    baby_tex: &'static str,
    armor: &'static str,
    baby_armor: &'static str,
    style: HumanoidStyle,
    outer: Option<(&'static str, &'static str, &'static str)>,
    baby_index: u8,
    kind: Kind,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Plain,
    Drowned,
    Skeleton,
    Piglin,
    Brute,
    ZombifiedPiglin,
    Vindicator,
    Pillager,
    Evoker,
    Illusioner,
}

const BASE: Hum = Hum {
    layer: "",
    baby_layer: "",
    tex: "",
    baby_tex: "",
    armor: "",
    baby_armor: "",
    style: HumanoidStyle::Plain,
    outer: None,
    baby_index: 255,
    kind: Kind::Plain,
};

static ZOMBIE: Hum = Hum {
    layer: "minecraft:zombie#main",
    baby_layer: "minecraft:zombie_baby#main",
    tex: "entity/zombie/zombie",
    baby_tex: "entity/zombie/zombie_baby",
    armor: "zombie",
    baby_armor: "zombie_baby",
    style: HumanoidStyle::Zombie,
    baby_index: idx::ZOMBIE_BABY,
    ..BASE
};
static HUSK: Hum = Hum {
    layer: "minecraft:husk#main",
    baby_layer: "minecraft:husk_baby#main",
    tex: "entity/zombie/husk",
    baby_tex: "entity/zombie/husk_baby",
    armor: "husk",
    baby_armor: "husk_baby",
    ..ZOMBIE
};
static DROWNED: Hum = Hum {
    layer: "minecraft:drowned#main",
    baby_layer: "minecraft:drowned_baby#main",
    tex: "entity/zombie/drowned",
    baby_tex: "entity/zombie/drowned_baby",
    armor: "drowned",
    baby_armor: "drowned_baby",
    outer: Some((
        "minecraft:drowned#outer",
        "entity/zombie/drowned_outer_layer",
        "entity/zombie/drowned_outer_layer_baby",
    )),
    style: HumanoidStyle::Drowned,
    kind: Kind::Drowned,
    ..ZOMBIE
};
static GIANT: Hum = Hum {
    layer: "minecraft:giant#main",
    baby_layer: "minecraft:giant#main",
    tex: "entity/zombie/zombie",
    baby_tex: "entity/zombie/zombie",
    armor: "giant",
    baby_armor: "giant",
    ..ZOMBIE
};
static ZOMBIFIED_PIGLIN: Hum = Hum {
    layer: "minecraft:zombified_piglin#main",
    baby_layer: "minecraft:zombified_piglin_baby#main",
    tex: "entity/piglin/zombified_piglin",
    baby_tex: "entity/piglin/zombified_piglin_baby",
    armor: "zombified_piglin",
    baby_armor: "zombified_piglin_baby",
    style: HumanoidStyle::ZombifiedPiglin,
    kind: Kind::ZombifiedPiglin,
    ..ZOMBIE
};
static PIGLIN: Hum = Hum {
    layer: "minecraft:piglin#main",
    baby_layer: "minecraft:piglin_baby#main",
    tex: "entity/piglin/piglin",
    baby_tex: "entity/piglin/piglin_baby",
    armor: "piglin",
    baby_armor: "piglin_baby",
    style: HumanoidStyle::Piglin,
    baby_index: idx::PIGLIN_BABY,
    kind: Kind::Piglin,
    ..BASE
};
static PIGLIN_BRUTE: Hum = Hum {
    layer: "minecraft:piglin_brute#main",
    baby_layer: "minecraft:piglin_brute#main",
    tex: "entity/piglin/piglin_brute",
    baby_tex: "entity/piglin/piglin_brute",
    armor: "piglin_brute",
    baby_armor: "piglin_brute",
    baby_index: 255,
    kind: Kind::Brute,
    ..PIGLIN
};
static SKELETON: Hum = Hum {
    layer: "minecraft:skeleton#main",
    baby_layer: "minecraft:skeleton#main",
    tex: "entity/skeleton/skeleton",
    baby_tex: "entity/skeleton/skeleton",
    armor: "skeleton",
    baby_armor: "skeleton",
    style: HumanoidStyle::Skeleton,
    kind: Kind::Skeleton,
    ..BASE
};
static STRAY: Hum = Hum {
    layer: "minecraft:stray#main",
    baby_layer: "minecraft:stray#main",
    tex: "entity/skeleton/stray",
    baby_tex: "entity/skeleton/stray",
    armor: "stray",
    baby_armor: "stray",
    outer: Some((
        "minecraft:stray#outer",
        "entity/skeleton/stray_overlay",
        "entity/skeleton/stray_overlay",
    )),
    ..SKELETON
};
static BOGGED: Hum = Hum {
    layer: "minecraft:bogged#main",
    baby_layer: "minecraft:bogged#main",
    tex: "entity/skeleton/bogged",
    baby_tex: "entity/skeleton/bogged",
    armor: "bogged",
    baby_armor: "bogged",
    outer: Some((
        "minecraft:bogged#outer",
        "entity/skeleton/bogged_overlay",
        "entity/skeleton/bogged_overlay",
    )),
    ..SKELETON
};
static WITHER_SKELETON: Hum = Hum {
    layer: "minecraft:wither_skeleton#main",
    baby_layer: "minecraft:wither_skeleton#main",
    tex: "entity/skeleton/wither_skeleton",
    baby_tex: "entity/skeleton/wither_skeleton",
    armor: "wither_skeleton",
    baby_armor: "wither_skeleton",
    ..SKELETON
};
static PARCHED: Hum = Hum {
    layer: "minecraft:parched#main",
    baby_layer: "minecraft:parched#main",
    tex: "entity/skeleton/parched",
    baby_tex: "entity/skeleton/parched",
    armor: "parched",
    baby_armor: "parched",
    ..SKELETON
};
static ENDERMAN: Hum = Hum {
    layer: "minecraft:enderman#main",
    baby_layer: "minecraft:enderman#main",
    tex: "entity/enderman/enderman",
    baby_tex: "entity/enderman/enderman",
    ..BASE
};
static EVOKER: Hum = Hum {
    layer: "minecraft:evoker#main",
    baby_layer: "minecraft:evoker#main",
    tex: "entity/illager/evoker",
    baby_tex: "entity/illager/evoker",
    style: HumanoidStyle::Illager,
    kind: Kind::Evoker,
    ..BASE
};
static ILLUSIONER: Hum = Hum {
    layer: "minecraft:illusioner#main",
    baby_layer: "minecraft:illusioner#main",
    tex: "entity/illager/illusioner",
    baby_tex: "entity/illager/illusioner",
    kind: Kind::Illusioner,
    ..EVOKER
};
static PILLAGER: Hum = Hum {
    layer: "minecraft:pillager#main",
    baby_layer: "minecraft:pillager#main",
    tex: "entity/illager/pillager",
    baby_tex: "entity/illager/pillager",
    kind: Kind::Pillager,
    ..EVOKER
};
static VINDICATOR: Hum = Hum {
    layer: "minecraft:vindicator#main",
    baby_layer: "minecraft:vindicator#main",
    tex: "entity/illager/vindicator",
    baby_tex: "entity/illager/vindicator",
    kind: Kind::Vindicator,
    ..EVOKER
};

pub(crate) fn builder(name: &str) -> Option<BuildFn> {
    Some(match name {
        "minecraft:zombie" => |c, v, p| hum(c, v, p, &ZOMBIE),
        "minecraft:husk" => |c, v, p| hum(c, v, p, &HUSK),
        "minecraft:drowned" => |c, v, p| hum(c, v, p, &DROWNED),
        "minecraft:giant" => |c, v, p| {
            p.extra_scale = Vec3::splat(1.0);
            hum(c, v, p, &GIANT)
        },
        "minecraft:zombified_piglin" => |c, v, p| hum(c, v, p, &ZOMBIFIED_PIGLIN),
        "minecraft:piglin" => |c, v, p| hum(c, v, p, &PIGLIN),
        "minecraft:piglin_brute" => |c, v, p| hum(c, v, p, &PIGLIN_BRUTE),
        "minecraft:skeleton" => |c, v, p| hum(c, v, p, &SKELETON),
        "minecraft:stray" => |c, v, p| hum(c, v, p, &STRAY),
        "minecraft:bogged" => |c, v, p| hum(c, v, p, &BOGGED),
        "minecraft:wither_skeleton" => |c, v, p| hum(c, v, p, &WITHER_SKELETON),
        "minecraft:parched" => |c, v, p| hum(c, v, p, &PARCHED),
        "minecraft:enderman" => |c, v, p| hum(c, v, p, &ENDERMAN),
        "minecraft:evoker" => |c, v, p| hum(c, v, p, &EVOKER),
        "minecraft:illusioner" => |c, v, p| hum(c, v, p, &ILLUSIONER),
        "minecraft:pillager" => |c, v, p| hum(c, v, p, &PILLAGER),
        "minecraft:vindicator" => |c, v, p| hum(c, v, p, &VINDICATOR),
        "minecraft:villager" => |c, v, p| villager(c, v, p, false),
        "minecraft:zombie_villager" => |c, v, p| villager(c, v, p, true),
        "minecraft:wandering_trader" => |_, _, p| {
            p.rig = Rig::Villager;
            p.push(Part::new(
                "minecraft:wandering_trader#main",
                "entity/wandering_trader/wandering_trader",
            ));
        },
        "minecraft:witch" => |_, _, p| {
            p.rig = Rig::Villager;
            p.push(Part::new("minecraft:witch#main", "entity/witch/witch"));
        },
        "minecraft:armor_stand" => armor_stand,
        "minecraft:player" | "minecraft:mannequin" => player,
        _ => return None,
    })
}

fn left_handed(v: &EntityView<'_>) -> bool {
    v.mob_flag(1)
}

/// Attaches main- and off-hand items at the arms and records handedness for the rigs.
fn attach_hands(
    v: &EntityView<'_>,
    plan: &mut Plan,
    family: HandFamily,
    left_handed: bool,
    baby: bool,
) {
    for (stack, left, is_main) in arm_items(v, left_handed) {
        if stack.is_none() {
            continue;
        }
        let equip = if is_main {
            slot::MAINHAND
        } else {
            slot::OFFHAND
        };
        let context = if left {
            ItemContext::LeftHand
        } else {
            ItemContext::RightHand
        };
        plan.attachments.push(hand_attach(
            family,
            left,
            baby,
            ItemSource::Equipment(equip),
            context,
        ));
        if is_spear(stack) {
            set_pose(plan, left, ArmPose::Spear);
        }
    }
}

fn set_handedness(v: &EntityView<'_>, plan: &mut Plan, left_handed: bool) {
    plan.anim.main_hand_empty = v.item(slot::MAINHAND).is_none();
    plan.anim.main_right = !left_handed;
    plan.anim.swing_left = left_handed;
}

fn attach_head_item(cx: &PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan, bone: &'static str) {
    let Some(stack) = v.item(slot::HEAD) else {
        return;
    };
    if crate::equipment::equippable(cx, stack).is_some_and(|(_, s)| s == "head") {
        return;
    }
    let pre = Mat4::from_scale(Vec3::splat(16.0))
        * Mat4::from_translation(Vec3::new(0.0, -0.25, 0.0))
        * Mat4::from_rotation_y(std::f32::consts::PI)
        * Mat4::from_scale(Vec3::new(0.625, -0.625, -0.625));
    plan.attachments.push(Attach::new(
        bone,
        AttachKind::Item {
            source: ItemSource::Equipment(slot::HEAD),
            context: ItemContext::Head,
        },
        pre,
    ));
}

fn casting(v: &EntityView<'_>) -> bool {
    v.byte(17).is_some_and(|c| c != 0)
}

fn illager_pose(c: &Hum, v: &EntityView<'_>, aggressive: bool) -> IllagerPose {
    let celebrating = v.boolean(16);
    match c.kind {
        Kind::Evoker => {
            if casting(v) {
                IllagerPose::Spellcasting
            } else if celebrating {
                IllagerPose::Celebrating
            } else {
                IllagerPose::Crossed
            }
        }
        Kind::Illusioner => {
            if casting(v) {
                IllagerPose::Spellcasting
            } else if aggressive {
                IllagerPose::BowAndArrow
            } else {
                IllagerPose::Crossed
            }
        }
        Kind::Vindicator => {
            if aggressive {
                IllagerPose::Attacking
            } else if celebrating {
                IllagerPose::Celebrating
            } else {
                IllagerPose::Neutral
            }
        }
        Kind::Pillager => {
            if v.boolean(idx::PILLAGER_CHARGING) {
                IllagerPose::CrossbowCharge
            } else if is_item(v.item(slot::MAINHAND), "minecraft:crossbow") {
                IllagerPose::CrossbowHold
            } else if aggressive {
                IllagerPose::Attacking
            } else {
                IllagerPose::Neutral
            }
        }
        _ => IllagerPose::Crossed,
    }
}

fn piglin_pose(kind: Kind, v: &EntityView<'_>, aggressive: bool) -> PiglinPose {
    let main = v.item(slot::MAINHAND);
    let crossbow = is_item(main, "minecraft:crossbow");
    let melee = main
        .and_then(item_name)
        .is_some_and(|n| n.ends_with("_sword") || n.ends_with("_axe"));
    if kind == Kind::Brute {
        return if aggressive && melee {
            PiglinPose::MeleeAttack
        } else {
            PiglinPose::Default
        };
    }
    if v.boolean(idx::PIGLIN_DANCING) {
        PiglinPose::Dancing
    } else if is_item(v.item(slot::OFFHAND), "minecraft:gold_ingot") {
        PiglinPose::Admiring
    } else if aggressive && melee {
        PiglinPose::MeleeAttack
    } else if v.boolean(idx::PIGLIN_CHARGING) {
        PiglinPose::CrossbowCharge
    } else if aggressive && crossbow {
        PiglinPose::CrossbowHold
    } else {
        PiglinPose::Default
    }
}

fn hum(cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan, c: &Hum) {
    let illager = matches!(
        c.kind,
        Kind::Evoker | Kind::Illusioner | Kind::Pillager | Kind::Vindicator
    );
    plan.rig = if illager {
        Rig::Illager
    } else {
        Rig::Humanoid(c.style)
    };
    let baby =
        c.baby_index != 255 && matches!(v.raw(c.baby_index), Some(EntityDataValue::Boolean(true)));
    plan.anim.baby = baby;
    let (layer, tex) = if baby {
        (c.baby_layer, c.baby_tex)
    } else {
        (c.layer, c.tex)
    };
    plan.push(Part::new(layer, tex));
    if let Some((l, t, bt)) = c.outer {
        plan.push(Part::new(l, if baby { bt } else { t }).overlay().no_hurt());
    }
    let lefty = left_handed(v);
    let aggressive = plan.anim.aggressive;
    let show_item = match c.kind {
        Kind::Vindicator => aggressive,
        Kind::Evoker => casting(v),
        Kind::Illusioner => aggressive || casting(v),
        _ => true,
    };
    if show_item {
        let family = if c.style == HumanoidStyle::Skeleton {
            HandFamily::Skeleton
        } else {
            HandFamily::Default
        };
        attach_hands(v, plan, family, lefty, baby);
    }
    set_handedness(v, plan, lefty);
    let main_item = v.item(slot::MAINHAND);
    match c.kind {
        Kind::Skeleton => {
            plan.anim.holding_bow = is_item(main_item, "minecraft:bow");
            if aggressive && plan.anim.holding_bow {
                set_pose(plan, lefty, ArmPose::BowAndArrow);
            }
        }
        Kind::Drowned => {
            if aggressive && is_item(main_item, "minecraft:trident") {
                set_pose(plan, lefty, ArmPose::ThrowTrident);
            }
        }
        Kind::Piglin | Kind::Brute => {
            plan.anim.piglin = piglin_pose(c.kind, v, aggressive);
            plan.anim.charge_max = 25.0;
            plan.anim.use_ticks = v.use_ticks;
        }
        Kind::Evoker | Kind::Illusioner | Kind::Pillager | Kind::Vindicator => {
            plan.anim.illager = illager_pose(c, v, aggressive);
            plan.anim.charge_max = 25.0;
            plan.anim.use_ticks = v.use_ticks;
            if c.kind != Kind::Illusioner {
                plan.hide.push(Slot::Hat);
            }
        }
        _ => {}
    }
    if !c.armor.is_empty() {
        let prefix = if baby { c.baby_armor } else { c.armor };
        humanoid_armor(cx, v, plan, prefix, baby);
        wings(cx, v, plan, baby);
    }
    attach_head_item(cx, v, plan, "head");
    if c.layer == "minecraft:enderman#main" {
        plan.push(Part::new("minecraft:enderman#main", "entity/enderman/enderman_eyes").eyes());
    }
}

fn villager(cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan, zombie: bool) {
    let baby = crate::catalog::is_baby(v)
        || (zombie
            && matches!(
                v.raw(idx::ZOMBIE_BABY),
                Some(EntityDataValue::Boolean(true))
            ));
    plan.anim.baby = baby;
    let data = if zombie {
        idx::ZOMBIE_VILLAGER_DATA
    } else {
        idx::VILLAGER_DATA
    };
    let (kind, prof, level) = match v.raw(data) {
        Some(EntityDataValue::VillagerData(d)) => (d.kind, d.profession, d.level),
        _ => (2, 0, 1),
    };
    const TYPES: [&str; 7] = [
        "desert", "jungle", "plains", "savanna", "snow", "swamp", "taiga",
    ];
    const PROFS: [&str; 15] = [
        "none",
        "armorer",
        "butcher",
        "cartographer",
        "cleric",
        "farmer",
        "fisherman",
        "fletcher",
        "leatherworker",
        "librarian",
        "mason",
        "nitwit",
        "shepherd",
        "toolsmith",
        "weaponsmith",
    ];
    let kind_name = cx
        .reg
        .entry_name("minecraft:villager_type", kind)
        .map(|n| short(n).to_owned())
        .unwrap_or_else(|| TYPES[kind.rem_euclid(7) as usize].to_owned());
    let prof_name = cx
        .reg
        .entry_name("minecraft:villager_profession", prof)
        .map(|n| short(n).to_owned())
        .unwrap_or_else(|| PROFS[prof.rem_euclid(15) as usize].to_owned());
    let dir = if zombie {
        "zombie_villager"
    } else {
        "villager"
    };
    let b = if baby { "_baby" } else { "" };
    let main = format!("minecraft:{dir}{b}#main");
    plan.rig = Rig::Villager;
    if zombie {
        plan.rig = Rig::Humanoid(HumanoidStyle::Zombie);
    }
    plan.push(Part::new(main.clone(), format!("entity/{dir}/{dir}{b}")));
    let type_tex = if baby {
        format!("entity/{dir}/baby/{kind_name}")
    } else {
        format!("entity/{dir}/type/{kind_name}")
    };
    plan.push(Part::new(main.clone(), type_tex).overlay().no_hurt());
    if !baby && prof_name != "none" {
        plan.push(
            Part::new(main.clone(), format!("entity/{dir}/profession/{prof_name}"))
                .overlay()
                .no_hurt(),
        );
        if prof_name != "nitwit" {
            let lv =
                ["stone", "iron", "gold", "emerald", "diamond"][(level.clamp(1, 5) - 1) as usize];
            plan.push(
                Part::new(main, format!("entity/{dir}/profession_level/{lv}"))
                    .overlay()
                    .no_hurt(),
            );
        }
    }
    if zombie {
        humanoid_armor(
            cx,
            v,
            plan,
            if baby {
                "zombie_villager_baby"
            } else {
                "zombie_villager"
            },
            baby,
        );
        wings(cx, v, plan, baby);
        let lefty = left_handed(v);
        attach_hands(v, plan, HandFamily::Default, lefty, baby);
        set_handedness(v, plan, lefty);
    }
    let _ = variant_name;
}

fn armor_stand(cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
    let flags = v.byte(idx::ARMOR_STAND_FLAGS).unwrap_or(0) as u8;
    let small = flags & 1 != 0;
    let arms = flags & 4 != 0;
    let no_base = flags & 8 != 0;
    plan.rig = Rig::Humanoid(HumanoidStyle::ArmorStand);
    plan.anim.baby = small;
    plan.body_yaw_offset = match v.events.wiggle {
        Some(w) if w < 5.0 => -(w / 1.5 * std::f32::consts::PI).sin() * 3.0,
        _ => 0.0,
    };
    let (layer, prefix) = if small {
        ("minecraft:armor_stand_small#main", "armor_stand_small")
    } else {
        ("minecraft:armor_stand#main", "armor_stand")
    };
    plan.push(Part::new(layer, "entity/armorstand/armorstand").no_hurt());
    if !arms {
        plan.hide.push(Slot::LeftArm);
        plan.hide.push(Slot::RightArm);
    }
    if no_base {
        plan.hide.push(Slot::BasePlate);
    }
    let pose = |i: u8, d: [f32; 3]| match v.raw(i) {
        Some(EntityDataValue::Rotations(r)) => [r.x, r.y, r.z],
        _ => d,
    };
    plan.rot_overrides
        .push((Slot::Head, pose(idx::ARMOR_STAND_HEAD_POSE, [0.0; 3])));
    plan.rot_overrides
        .push((Slot::Body, pose(idx::ARMOR_STAND_BODY_POSE, [0.0; 3])));
    plan.rot_overrides.push((
        Slot::LeftArm,
        pose(idx::ARMOR_STAND_LEFT_ARM_POSE, [-10.0, 0.0, -10.0]),
    ));
    plan.rot_overrides.push((
        Slot::RightArm,
        pose(idx::ARMOR_STAND_RIGHT_ARM_POSE, [-15.0, 0.0, 10.0]),
    ));
    plan.rot_overrides.push((
        Slot::LeftLeg,
        pose(idx::ARMOR_STAND_LEFT_LEG_POSE, [-1.0, 0.0, -1.0]),
    ));
    plan.rot_overrides.push((
        Slot::RightLeg,
        pose(idx::ARMOR_STAND_RIGHT_LEG_POSE, [1.0, 0.0, 1.0]),
    ));
    humanoid_armor(cx, v, plan, prefix, false);
    wings(cx, v, plan, small);
    attach_hands(v, plan, HandFamily::Default, false, false);
    attach_head_item(cx, v, plan, "head");
}

const DEFAULT_SKINS: [&str; 9] = [
    "alex", "ari", "efe", "kai", "makena", "noor", "steve", "sunny", "zuri",
];

/// Vanilla's `DefaultPlayerSkin.get(uuid)`: index into 9 slim skins followed by 9 wide skins.
pub fn default_skin(uuid: Uuid) -> (&'static str, bool) {
    let (hi, lo) = uuid.as_u64_pair();
    let x = hi ^ lo;
    let hash = ((x >> 32) as i32) ^ (x as i32);
    let index = hash.rem_euclid(18) as usize;
    (DEFAULT_SKINS[index % 9], index < 9)
}

fn player(cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
    let pv = &v.player;
    let (texture, slim) = match pv.skin {
        Some(skin) => {
            let key = format!("dyn:skin:{}", skin.key);
            let (w, h, rgba) = (skin.width, skin.height, &skin.rgba);
            cx.tex.register(&key, || crate::skin::prepare(w, h, rgba));
            (key, skin.slim)
        }
        None => {
            let (name, slim) = default_skin(v.uuid);
            (
                format!(
                    "entity/player/{}/{name}",
                    if slim { "slim" } else { "wide" }
                ),
                slim,
            )
        }
    };
    plan.rig = Rig::Humanoid(HumanoidStyle::Plain);
    let model = if slim {
        "minecraft:player_slim#main"
    } else {
        "minecraft:player#main"
    };
    plan.extra_scale = Vec3::splat(0.9375);
    let crouching = v.pose() == mcr_protocol::types::entity_data::Pose::Crouching;
    if crouching {
        plan.extra_offset = Vec3::new(0.0, -0.125, 0.0);
    }
    let mask = if pv.model_parts == 0 && v.raw(idx::PLAYER_MODEL_PARTS).is_none() {
        0x7F
    } else if v.raw(idx::PLAYER_MODEL_PARTS).is_some() {
        v.byte(idx::PLAYER_MODEL_PARTS).unwrap_or(0) as u8
    } else {
        pv.model_parts
    };
    plan.push(Part::new(model, texture.clone()));
    for (bit, slots) in [
        (1u8 << 1, Slot::Jacket),
        (1 << 2, Slot::LeftSleeve),
        (1 << 3, Slot::RightSleeve),
        (1 << 4, Slot::LeftPants),
        (1 << 5, Slot::RightPants),
        (1 << 6, Slot::Hat),
    ] {
        if mask & bit == 0 {
            plan.hide.push(slots);
        }
    }
    let left_handed = matches!(
        v.raw(idx::PLAYER_MAIN_HAND),
        Some(EntityDataValue::HumanoidArm(
            mcr_protocol::types::entity_data::HumanoidArm::Left
        ))
    ) || pv.left_handed;
    set_handedness(v, plan, left_handed);
    let family = if slim {
        HandFamily::Slim
    } else {
        HandFamily::Default
    };
    attach_hands(v, plan, family, left_handed, false);
    player_poses(v, plan, left_handed);
    attach_head_item(cx, v, plan, "head");
    let prefix = if slim { "player_slim" } else { "player" };
    humanoid_armor(cx, v, plan, prefix, false);
    wings(cx, v, plan, false);
    avatar_layers(cx, v, plan, &texture, slim, mask & 1 != 0);
    plan.post_rot = avatar_rotation(v);
    let _ = hitbox;
}

const MAX_STUCK: i32 = 64;

/// The cubes `PlayerModel.getRandomBodyPart` picks from: bone, min and max corner in sixteenths.
fn body_cubes(slim: bool) -> [(&'static str, Vec3, Vec3); 6] {
    let arm = if slim { 3.0 } else { 4.0 };
    [
        (
            "head",
            Vec3::new(-4.0, -8.0, -4.0),
            Vec3::new(4.0, 0.0, 4.0),
        ),
        (
            "body",
            Vec3::new(-4.0, 0.0, -2.0),
            Vec3::new(4.0, 12.0, 2.0),
        ),
        (
            "left_arm",
            Vec3::new(-1.0, -2.0, -2.0),
            Vec3::new(-1.0 + arm, 10.0, 2.0),
        ),
        (
            "right_arm",
            Vec3::new(1.0 - arm, -2.0, -2.0),
            Vec3::new(1.0, 10.0, 2.0),
        ),
        (
            "left_leg",
            Vec3::new(-2.0, 0.0, -2.0),
            Vec3::new(2.0, 12.0, 2.0),
        ),
        (
            "right_leg",
            Vec3::new(-2.0, 0.0, -2.0),
            Vec3::new(2.0, 12.0, 2.0),
        ),
    ]
}

/// `StuckInBodyLayer`: where each stuck arrow or stinger sits, seeded by the entity id.
pub(crate) fn stuck_frames(
    id: i32,
    count: i32,
    slim: bool,
    surface: bool,
) -> Vec<(&'static str, Mat4)> {
    let mut rng = crate::effects::JavaRandom::new(i64::from(id) as u64);
    let cubes = body_cubes(slim);
    (0..count.clamp(0, MAX_STUCK))
        .map(|_| {
            let (bone, min, max) = cubes[rng.next_int(6) as usize];
            rng.next_int(1);
            let mut mid = [rng.next_float(), rng.next_float(), rng.next_float()];
            if surface {
                let plane = rng.next_int(3) as usize;
                mid[plane] = if mid[plane] > 0.5 { 1.0 } else { 0.5 };
            }
            let mid = Vec3::from(mid);
            let pos = min + (max - min) * mid;
            let dir = -(mid * 2.0 - Vec3::ONE);
            let y_rot = dir.x.atan2(dir.z);
            let x_rot = dir.y.atan2(dir.x.hypot(dir.z));
            let local = Mat4::from_translation(pos)
                * Mat4::from_rotation_y(y_rot - std::f32::consts::FRAC_PI_2)
                * Mat4::from_rotation_z(x_rot);
            (bone, local)
        })
        .collect()
}

/// `PlayerCapeModel.setupAnim`: the cape's final rotation as ZYX euler degrees.
pub(crate) fn cape_rotation([flap, lean, lean2]: [f32; 3]) -> [f32; 3] {
    let q = glam::Quat::from_rotation_x((6.0 + lean / 2.0 + flap).to_radians())
        * glam::Quat::from_rotation_z((lean2 / 2.0).to_radians())
        * glam::Quat::from_rotation_y((180.0 - lean2 / 2.0).to_radians());
    let (z, y, x) = q.to_euler(glam::EulerRot::ZYX);
    [x.to_degrees(), y.to_degrees(), z.to_degrees()]
}

/// The extra layers of `AvatarRenderer`: cape, deadmau5 ears, stuck arrows and stingers, shoulder
/// parrots and the riptide swirl.
fn avatar_layers(
    cx: &mut PlanCx<'_>,
    v: &EntityView<'_>,
    plan: &mut Plan,
    skin_texture: &str,
    slim: bool,
    show_cape: bool,
) {
    let pv = &v.player;
    let chest = v.item(slot::CHEST);
    if !v.invisible
        && show_cape
        && let Some(cape) = pv.skin.and_then(|s| s.cape.as_ref())
        && !chest.is_some_and(|c| has_layer(cx, c, "wings"))
    {
        let key = format!("dyn:cape:{}", cape.key);
        let (w, h, rgba) = (cape.width, cape.height, &cape.rgba);
        cx.tex
            .register(&key, || crate::skin::prepare_cape(w, h, rgba));
        let armored = chest.is_some_and(|c| has_layer(cx, c, "humanoid"));
        let part = plan.push(Part::new("minecraft:player#cape", key).no_hurt());
        if armored {
            part.offset = Vec3::new(0.0, -0.85, 1.1);
        }
        plan.rot_overrides
            .push((Slot::Cape, cape_rotation(pv.cloak)));
    }
    if pv.extra_ears && !v.invisible {
        plan.push(Part::new("minecraft:player#ears", skin_texture.to_owned()));
    }
    for (index, layer, texture, surface) in [
        (
            idx::LIVING_ARROWS,
            "minecraft:arrow#main",
            "entity/projectiles/arrow",
            false,
        ),
        (
            idx::LIVING_STINGERS,
            "minecraft:bee_stinger#main",
            "entity/bee/bee_stinger",
            true,
        ),
    ] {
        let count = v.int(index).unwrap_or(0);
        for (bone, local) in stuck_frames(v.id, count, slim, surface) {
            let part = plan.push(Part::new(layer, texture).no_hurt().not_body());
            part.pose = crate::plan::Pose::Rest;
            part.anchor = Some((bone, local));
        }
    }
    let crouching = v.pose() == Pose::Crouching;
    for (index, x) in [
        (idx::PLAYER_SHOULDER_LEFT, 6.4),
        (idx::PLAYER_SHOULDER_RIGHT, -6.4),
    ] {
        let Some(EntityDataValue::OptionalUnsignedInt(Some(variant))) = v.raw(index) else {
            continue;
        };
        let texture = crate::mobs::parrot_texture(i32::try_from(*variant).unwrap_or(0));
        let part = plan.push(
            Part::new("minecraft:parrot#main", texture)
                .no_hurt()
                .not_body(),
        );
        part.pose = crate::plan::Pose::Own(Rig::ShoulderParrot);
        part.offset = Vec3::new(x, if crouching { -20.8 } else { -24.0 }, 0.0);
    }
    if v.living_flag(2) {
        let part = plan.push(
            Part::new(
                "minecraft:spin_attack#main",
                "entity/trident/trident_riptide",
            )
            .no_hurt()
            .not_body(),
        );
        part.pose = crate::plan::Pose::Own(Rig::SpinAttack);
    }
}

/// `AvatarRenderer.setupRotations` on top of the living turn: riptide spin, glide pitch and roll, swim.
pub(crate) fn avatar_rotation(v: &EntityView<'_>) -> Mat4 {
    let spin = v.living_flag(2);
    let mut m = Mat4::IDENTITY;
    if spin {
        m = Mat4::from_rotation_x((-90.0 - v.pitch).to_radians())
            * Mat4::from_rotation_y((v.age_ticks * -75.0).to_radians());
    }
    if v.shared_flag(7) || v.pose() == Pose::FallFlying {
        if !spin {
            m *= Mat4::from_rotation_x((v.glide.scale() * (-90.0 - v.pitch)).to_radians());
        }
        if let Some(y) = v.glide.y_rot {
            m *= Mat4::from_rotation_y(y);
        }
    } else if v.pose() == Pose::Swimming && !spin {
        let x = if v.shared_flag(4) {
            -90.0 - v.pitch
        } else {
            -90.0
        };
        m *= Mat4::from_rotation_x(x.to_radians())
            * Mat4::from_translation(Vec3::new(0.0, -1.0, 0.3));
    }
    m
}

/// `WingsLayer` with `ElytraModel.setupAnim`: 2px behind the body, lowered while crouching.
fn wings(cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan, baby: bool) {
    let Some(chest) = v.item(slot::CHEST) else {
        return;
    };
    let model = if baby {
        "minecraft:elytra_baby#main"
    } else {
        "minecraft:elytra#main"
    };
    let before = plan.parts.len();
    if !add_layers(cx, plan, chest, "chest", "wings", model) {
        return;
    }
    let y = if v.pose() == Pose::Crouching {
        3.0
    } else {
        0.0
    };
    for part in &mut plan.parts[before..] {
        part.offset = Vec3::new(0.0, y, 2.0);
    }
    let [x, ry, z] = v.glide.wings.map(f32::to_degrees);
    plan.rot_overrides.push((Slot::LeftWing, [x, ry, z]));
    plan.rot_overrides.push((Slot::RightWing, [x, -ry, -z]));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stuck_arrows_are_seeded_and_bounded() {
        let a = stuck_frames(42, 5, false, false);
        assert_eq!(a.len(), 5);
        let b = stuck_frames(42, 5, false, false);
        assert_eq!(a, b);
        assert_ne!(a, stuck_frames(43, 5, false, false));
        assert_eq!(
            stuck_frames(1, i32::MAX, true, true).len(),
            MAX_STUCK as usize
        );
        assert!(stuck_frames(1, -3, false, false).is_empty());
        for (bone, m) in stuck_frames(9, 20, true, true) {
            let (_, min, max) = body_cubes(true)
                .into_iter()
                .find(|(b, _, _)| *b == bone)
                .unwrap();
            let p = m.w_axis.truncate();
            assert!(
                p.cmpge(min - 1e-4).all() && p.cmple(max + 1e-4).all(),
                "{bone} {p}"
            );
        }
    }

    #[test]
    fn still_cape_hangs_six_degrees_out() {
        let [x, y, z] = cape_rotation([0.0; 3]);
        let q = glam::Quat::from_euler(
            glam::EulerRot::ZYX,
            z.to_radians(),
            y.to_radians(),
            x.to_radians(),
        );
        let want = glam::Quat::from_rotation_x(6f32.to_radians())
            * glam::Quat::from_rotation_y(std::f32::consts::PI);
        assert!(q.dot(want).abs() > 0.9999, "{x} {y} {z}");
    }
}
