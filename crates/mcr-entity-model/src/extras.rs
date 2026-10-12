use std::f32::consts::PI;

use glam::{Mat4, Vec3};
use mcr_protocol::types::entity_data::EntityDataValue;

use crate::catalog::wrap_degrees;
use crate::equipment::component;
use crate::hands::{HandFamily, hand_attach};
use crate::idx;
use crate::misc::state_by_name;
use crate::plan::{Attach, AttachKind, ItemContext, ItemSource, Part, Plan};
use crate::rig::{ArmPose, Rig, Slot};
use crate::view::{EntityView, slot};

const BLOCKS: f32 = 16.0;

fn blocks() -> Mat4 {
    Mat4::from_scale(Vec3::splat(BLOCKS))
}

fn item_attach(plan: &mut Plan, bone: &'static str, pre: Mat4, equip: usize) {
    plan.attachments.push(Attach::new(
        bone,
        AttachKind::Item {
            source: ItemSource::Equipment(equip),
            context: ItemContext::Ground,
        },
        pre,
    ));
}

fn block_attach(plan: &mut Plan, bone: &'static str, state: i32, pre: Mat4) {
    plan.attachments
        .push(Attach::new(bone, AttachKind::Block { state }, pre));
}

fn flags(v: &EntityView<'_>, index: u8) -> u8 {
    v.byte(index).unwrap_or(0) as u8
}

fn crossed_arms_item(plan: &mut Plan, v: &EntityView<'_>) {
    if v.item(slot::MAINHAND).is_none() {
        return;
    }
    let pre = blocks()
        * Mat4::from_rotation_x(0.75)
        * Mat4::from_scale(Vec3::splat(1.07))
        * Mat4::from_translation(Vec3::new(0.0, 0.13, -0.34))
        * Mat4::from_rotation_x(PI);
    item_attach(plan, "arms", pre, slot::MAINHAND);
}

fn fish_wobble(plan: &mut Plan, v: &EntityView<'_>, amplitude: f32, angle: f32, flop: Vec3) {
    plan.spin = (amplitude * 4.3 * (angle * 0.6 * v.age_ticks).sin()).to_radians();
    if !plan.anim.in_water {
        plan.post_rot = Mat4::from_translation(flop) * Mat4::from_rotation_z(90f32.to_radians());
    }
}

/// Per-type state that feeds the rigs and the layers vanilla draws beside the model.
pub fn decorate(deco: Deco, name: &str, v: &EntityView<'_>, plan: &mut Plan) {
    if deco == Deco::None {
        return;
    }
    let age = v.age_ticks;
    plan.anim.entity_id = v.id;
    plan.anim.in_water = !v.on_ground;
    match deco {
        Deco::Cod => {
            plan.rig = Rig::Fish;
            fish_wobble(plan, v, 1.0, 1.0, Vec3::new(0.1, 0.1, -0.1));
        }
        Deco::Salmon => {
            plan.rig = Rig::Fish;
            let (amp, ang) = if plan.anim.in_water {
                (1.0, 1.0)
            } else {
                (1.3, 1.7)
            };
            fish_wobble(plan, v, amp, ang, Vec3::new(0.2, 0.1, 0.0));
        }
        Deco::TropicalFish => {
            plan.rig = Rig::Fish;
            fish_wobble(plan, v, 1.0, 1.0, Vec3::new(0.2, 0.1, 0.0));
        }
        Deco::Pufferfish => {
            plan.rig = Rig::Pufferfish;
            plan.extra_offset.y += (age * 0.05).cos() * 0.08;
        }
        Deco::Squid => {
            plan.rig = Rig::Squid;
            let phase = v
                .events
                .tentacle_sync
                .map_or(age * 0.1 + v.id as f32 * 0.37, |t| t * 0.1);
            let t = phase % (PI * 2.0);
            plan.anim.tentacle = if t < PI {
                let k = t / PI;
                (k * k * PI).sin() * PI * 0.25
            } else {
                0.0
            };
            plan.extra_offset.y += if plan.anim.baby { -0.35 } else { -0.7 };
        }
        Deco::Phantom => {
            plan.rig = Rig::Phantom;
            let size = v.int(idx::PHANTOM_SIZE).unwrap_or(0).clamp(0, 64) as f32;
            let s = 1.0 + 0.15 * size;
            plan.anim.flap = (v.id as f32 * 3.0) + age;
            plan.extra_scale = Vec3::splat(s);
            plan.extra_offset += Vec3::new(0.0, -1.3125, 0.1875) * s;
            plan.post_rot = Mat4::from_rotation_x(v.pitch.to_radians());
            plan.push(Part::new("minecraft:phantom#main", "entity/phantom/phantom_eyes").eyes());
        }
        Deco::Guardian => {
            plan.rig = Rig::Guardian;
            plan.anim.holding = 1.0;
            plan.anim.tentacle = age * 0.45;
        }
        Deco::Dolphin => {
            plan.rig = Rig::Dolphin;
            plan.anim.charging = v.limb_amount > 0.01;
            if v.item(slot::MAINHAND).is_some() {
                let k = v.pitch.abs() / 60.0;
                let (y, z) = if v.pitch < 0.0 {
                    (1.0 - k * 0.5, -1.0 + k * 0.5)
                } else {
                    (1.0 + k * 0.8, -1.0 + k * 0.2)
                };
                item_attach(
                    plan,
                    "root",
                    blocks() * Mat4::from_translation(Vec3::new(0.0, y, z)),
                    slot::MAINHAND,
                );
            }
        }
        Deco::Hoglin => {
            plan.rig = Rig::Hoglin;
            plan.anim.attack_ticks = v.attack_ticks;
        }
        Deco::Ravager => {
            plan.rig = Rig::Ravager;
            plan.anim.attack_ticks = v.attack_ticks;
            plan.anim.stunned = v.events.stunned;
            plan.anim.roar = v.events.roar;
        }
        Deco::Goat => {
            if v.events.ram > 0.0 {
                let max = if plan.anim.baby { 52.5 } else { 30.0 };
                plan.anim.head_x = Some(v.events.ram / 20.0 * max * PI / 180.0);
            }
        }
        Deco::Sheep => {
            let (pos, angle) = sheep_head(v.events.eat);
            let age_scale = if plan.anim.baby { 0.5 } else { 1.0 };
            plan.anim.head_dy = pos * 9.0 * age_scale;
            plan.anim.head_x = angle;
        }
        Deco::Turtle => {
            plan.rig = Rig::Turtle;
            plan.anim.on_land = v.on_ground;
            let egg = v.boolean(idx::TURTLE_EGG);
            plan.anim.has_chest = egg && !plan.anim.baby;
            plan.anim.charging = v.boolean(19);
            if !egg {
                plan.hide.push(Slot::EggBelly);
            }
        }
        Deco::PolarBear => {
            plan.rig = Rig::Polar;
            plan.anim.stand = if v.boolean(idx::POLAR_BEAR_STANDING) {
                1.0
            } else {
                0.0
            };
        }
        Deco::Strider => plan.rig = Rig::Strider,
        Deco::SnowGolem => {
            plan.rig = Rig::SnowGolem;
            let pumpkin = v
                .byte(idx::SNOW_GOLEM_PUMPKIN)
                .is_none_or(|f| f as u8 & 16 != 0);
            if pumpkin {
                let pre = blocks()
                    * Mat4::from_translation(Vec3::new(0.0, -0.34375, 0.0))
                    * Mat4::from_rotation_y(PI)
                    * Mat4::from_scale(Vec3::new(0.625, -0.625, -0.625))
                    * Mat4::from_translation(Vec3::splat(-0.5));
                block_attach(plan, "head", state_by_name("minecraft:carved_pumpkin"), pre);
            }
        }
        Deco::IronGolem => {
            plan.rig = Rig::IronGolem;
            plan.anim.attack_ticks = v.attack_ticks;
            plan.anim.offer_ticks = v.offer_ticks;
            if v.offer_ticks > 0.0 {
                let pre = blocks()
                    * Mat4::from_translation(Vec3::new(-1.1875, 1.0625, -0.9375))
                    * Mat4::from_translation(Vec3::splat(0.5))
                    * Mat4::from_scale(Vec3::splat(0.5))
                    * Mat4::from_rotation_x(-90f32.to_radians())
                    * Mat4::from_translation(Vec3::splat(-0.5));
                block_attach(plan, "right_arm", state_by_name("minecraft:poppy"), pre);
            }
        }
        Deco::Mooshroom => {
            if !plan.anim.baby {
                let brown = v.int(idx::MOOSHROOM_TYPE) == Some(1);
                let state = state_by_name(if brown {
                    "minecraft:brown_mushroom"
                } else {
                    "minecraft:red_mushroom"
                });
                let flip = Mat4::from_scale(Vec3::new(-1.0, -1.0, 1.0))
                    * Mat4::from_translation(Vec3::splat(-0.5));
                let a = blocks()
                    * Mat4::from_translation(Vec3::new(0.2, -0.35, 0.5))
                    * Mat4::from_rotation_y(-48f32.to_radians())
                    * flip;
                let b = blocks()
                    * Mat4::from_translation(Vec3::new(0.2, -0.35, 0.5))
                    * Mat4::from_rotation_y(42f32.to_radians())
                    * Mat4::from_translation(Vec3::new(0.1, 0.0, -0.6))
                    * Mat4::from_rotation_y(-48f32.to_radians())
                    * flip;
                let c = blocks()
                    * Mat4::from_translation(Vec3::new(0.0, -0.7, -0.2))
                    * Mat4::from_rotation_y(-78f32.to_radians())
                    * flip;
                block_attach(plan, "root", state, a);
                block_attach(plan, "root", state, b);
                block_attach(plan, "head", state, c);
            }
        }
        Deco::Enderman => {
            plan.rig = Rig::Enderman;
            plan.anim.creepy = v.boolean(idx::ENDERMAN_CREEPY);
            if let Some(EntityDataValue::OptionalBlockState(Some(state))) =
                v.raw(idx::ENDERMAN_CARRY)
            {
                plan.anim.carrying = true;
                let pre = blocks()
                    * Mat4::from_translation(Vec3::new(0.0, 0.6875, -0.75))
                    * Mat4::from_rotation_x(20f32.to_radians())
                    * Mat4::from_rotation_y(45f32.to_radians())
                    * Mat4::from_translation(Vec3::new(0.25, 0.1875, 0.25))
                    * Mat4::from_scale(Vec3::new(-0.5, -0.5, 0.5))
                    * Mat4::from_rotation_y(90f32.to_radians());
                block_attach(plan, "root", *state, pre);
            }
        }
        Deco::Witch => {
            plan.rig = Rig::Witch;
            let drinking = v.boolean(idx::WITCH_USING_ITEM);
            plan.anim.item_held = drinking;
            if drinking {
                if v.item(slot::MAINHAND).is_some() {
                    let pre = blocks()
                        * Mat4::from_translation(Vec3::new(0.0625, 0.25, 0.0))
                        * Mat4::from_rotation_z(PI)
                        * Mat4::from_rotation_x(140f32.to_radians())
                        * Mat4::from_rotation_z(10f32.to_radians())
                        * Mat4::from_rotation_x(PI);
                    item_attach(plan, "nose", pre, slot::MAINHAND);
                }
            } else {
                crossed_arms_item(plan, v);
            }
        }
        Deco::Villager => crossed_arms_item(plan, v),
        Deco::Fox => {
            plan.rig = Rig::Fox;
            let f = flags(v, idx::FOX_FLAGS);
            plan.anim.sitting = f & 1 != 0;
            plan.anim.crouching = f & 4 != 0;
            plan.anim.sleeping = f & 32 != 0;
            plan.anim.crouch_amount = if plan.anim.crouching { 3.0 } else { 0.0 };
            plan.anim.head_roll = if f & 8 != 0 { 0.11 * PI } else { 0.0 };
            if v.item(slot::MAINHAND).is_some() {
                let baby = plan.anim.baby;
                let sleeping = plan.anim.sleeping;
                let shift = match (baby, sleeping) {
                    (true, true) => Vec3::new(0.4, 0.26, 0.15),
                    (true, false) => Vec3::new(0.06, 0.26, -0.5),
                    (false, true) => Vec3::new(0.46, 0.26, 0.22),
                    (false, false) => Vec3::new(0.06, 0.27, -0.5),
                };
                let mut pre = blocks();
                if baby {
                    pre *= Mat4::from_scale(Vec3::splat(0.75));
                }
                pre *= Mat4::from_translation(shift) * Mat4::from_rotation_x(90f32.to_radians());
                if sleeping {
                    pre *= Mat4::from_rotation_z(90f32.to_radians());
                }
                item_attach(plan, "head", pre, slot::MAINHAND);
            }
        }
        Deco::Panda => {
            plan.rig = Rig::Panda;
            let f = flags(v, idx::PANDA_FLAGS);
            let sitting = f & 8 != 0;
            let eating = v.int(idx::PANDA_EAT).unwrap_or(0) > 0;
            plan.anim.unhappy = v.int(idx::PANDA_UNHAPPY).unwrap_or(0) > 0;
            plan.anim.sneeze = if f & 2 != 0 {
                v.int(idx::PANDA_SNEEZE).unwrap_or(0) as f32
            } else {
                0.0
            };
            plan.anim.sit_amount = if sitting { 1.0 } else { 0.0 };
            plan.anim.lie_on_back = if f & 16 != 0 { 1.0 } else { 0.0 };
            plan.anim.roll = if f & 4 != 0 { 1.0 } else { 0.0 };
            plan.anim.eat = if eating { 1.0 } else { 0.0 };
            if sitting && v.item(slot::MAINHAND).is_some() {
                let (mut y, mut z) = (1.4, -0.6);
                if eating {
                    z -= 0.2 * (age * 0.6).sin() + 0.2;
                    y -= 0.09 * (age * 0.6).sin();
                }
                let pre = blocks() * Mat4::from_translation(Vec3::new(0.1, y, z));
                item_attach(plan, "root", pre, slot::MAINHAND);
            }
        }
        Deco::Parrot => {
            plan.rig = Rig::Parrot;
            plan.anim.sitting = flags(v, idx::TAMABLE_FLAGS) & 1 != 0;
            let speed = if v.on_ground { 0.0 } else { 1.0 };
            plan.anim.flap = ((age * 1.5).sin() + 1.0) * speed;
        }
        Deco::Bee => {
            plan.rig = Rig::Bee;
            let f = flags(v, idx::BEE_FLAGS);
            plan.anim.has_stinger = f & 4 == 0;
            plan.anim.roll = if f & 2 != 0 { 1.0 } else { 0.0 };
            plan.anim.aggressive = matches!(v.raw(19), Some(EntityDataValue::Long(t)) if *t > 0);
        }
        Deco::Horse => {
            plan.rig = Rig::Equine;
            let f = flags(v, idx::HORSE_FLAGS);
            plan.anim.eat = if f & 16 != 0 { 1.0 } else { 0.0 };
            plan.anim.stand = if f & 32 != 0 { 1.0 } else { 0.0 };
            plan.anim.feed = if f & 64 != 0 { 1.0 } else { 0.0 };
            plan.anim.donkey = matches!(name, "donkey" | "mule");
        }
        Deco::Llama => plan.rig = Rig::Llama,
        Deco::Wolf => {
            let (_, tame) = crate::mobs::tamable_flags(v);
            let angry = matches!(v.raw(idx::WOLF_ANGER), Some(EntityDataValue::Long(t)) if *t > 0);
            plan.anim.aggressive = angry;
            plan.anim.shake = v.shake;
            plan.anim.tail_angle = if angry {
                1.5393804
            } else if tame {
                let p = (v.float(9).unwrap_or(40.0) / 40.0).clamp(0.0, 1.0);
                (0.55 - (1.0 - p) * 0.4) * PI
            } else {
                PI / 5.0
            };
            if v.boolean(20) {
                plan.anim.head_roll = 0.15 * PI;
            }
        }
        Deco::Slime => {
            let size = v.int(idx::SLIME_SIZE).unwrap_or(1).clamp(1, 127) as f32;
            let squish = if v.squish != 0.0 {
                v.squish
            } else if v.on_ground {
                0.0
            } else {
                0.6
            };
            plan.anim.squish = squish;
            if name == "magma_cube" {
                plan.rig = Rig::Slime;
            }
            let ss = squish / (size * 0.5 + 1.0);
            let w = 1.0 / (ss + 1.0);
            plan.extra_scale = Vec3::new(w * size, size / w, w * size) * 0.999;
        }
        Deco::Vex => {
            plan.rig = Rig::Vex;
            plan.anim.charging = flags(v, idx::VEX_FLAGS) & 1 != 0;
            let lefty = v.mob_flag(1);
            plan.anim.item_held = v.item(slot::MAINHAND).is_some();
            plan.anim.main_right = !lefty;
            attach_flying_hands(plan, v, HandFamily::Vex, lefty);
        }
        Deco::Allay => {
            plan.rig = Rig::Allay;
            plan.anim.charging = v.boolean(idx::ALLAY_DANCING);
            plan.anim.holding = if v.item(slot::MAINHAND).is_some() {
                1.0
            } else {
                0.0
            };
            attach_flying_hands(plan, v, HandFamily::Allay, false);
        }
        Deco::CopperGolem => {
            let idle = !matches!(
                v.raw(idx::COPPER_GOLEM_STATE),
                Some(EntityDataValue::CopperGolemState(s)) if *s != mcr_protocol::types::entity_data::CopperGolemState::Idle
            );
            attach_flying_hands(plan, v, HandFamily::CopperGolem { idle }, false);
        }
        Deco::Shulker => {
            plan.rig = Rig::Shulker;
            plan.anim.peek = v.byte(idx::SHULKER_PEEK).unwrap_or(0).clamp(0, 100) as f32 / 100.0;
            plan.body_yaw_offset = 180.0;
            plan.anim.head_yaw = wrap_degrees(v.head_yaw - v.body_yaw - 180.0);
            let face = match v.raw(16) {
                Some(EntityDataValue::Direction(d)) => d.id(),
                _ => 0,
            };
            plan.post_rot = shulker_rotation(face as u8);
        }
        Deco::Wither => plan.rig = Rig::Wither,
        Deco::EnderDragon => {
            plan.rig = Rig::Dragon;
            plan.anim.flap = age * 0.06;
            plan.body_yaw_offset = 180.0;
            plan.extra_offset = Vec3::new(0.0, 0.0, 1.0);
        }
        Deco::EndCrystal => {
            plan.rig = Rig::Crystal;
            plan.anim.has_chest = v.boolean(idx::END_CRYSTAL_SHOW_BOTTOM)
                || v.raw(idx::END_CRYSTAL_SHOW_BOTTOM).is_none();
            plan.anim.flap = age;
            plan.unflipped(2.0);
            plan.extra_offset = Vec3::new(0.0, -1.0, 0.0);
        }
        Deco::None => {}
    }
    let _ = (ArmPose::Empty, component);
}

fn attach_flying_hands(plan: &mut Plan, v: &EntityView<'_>, family: HandFamily, lefty: bool) {
    for (equip, left) in [(slot::MAINHAND, lefty), (slot::OFFHAND, !lefty)] {
        if v.item(equip).is_none() {
            continue;
        }
        let context = if left {
            ItemContext::LeftHand
        } else {
            ItemContext::RightHand
        };
        plan.attachments.push(hand_attach(
            family,
            left,
            false,
            ItemSource::Equipment(equip),
            context,
        ));
    }
}

/// `attachFace.getOpposite().getRotation()` about (0, 0.5, 0) for the six wall faces.
fn shulker_rotation(face: u8) -> Mat4 {
    let half = std::f32::consts::FRAC_PI_2;
    let rot = match face {
        0 => Mat4::IDENTITY,
        1 => Mat4::from_rotation_x(PI),
        2 => Mat4::from_rotation_x(half),
        3 => Mat4::from_rotation_x(half) * Mat4::from_rotation_z(PI),
        4 => Mat4::from_rotation_x(half) * Mat4::from_rotation_z(-half),
        _ => Mat4::from_rotation_x(half) * Mat4::from_rotation_z(half),
    };
    let pivot = Vec3::new(0.0, 0.5, 0.0);
    Mat4::from_translation(pivot) * rot * Mat4::from_translation(-pivot)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Deco {
    None,
    Cod,
    Salmon,
    TropicalFish,
    Pufferfish,
    Squid,
    Phantom,
    Guardian,
    Dolphin,
    Hoglin,
    Ravager,
    Turtle,
    PolarBear,
    Strider,
    SnowGolem,
    IronGolem,
    Mooshroom,
    Enderman,
    Witch,
    Villager,
    Fox,
    Panda,
    Parrot,
    Bee,
    Horse,
    Llama,
    Wolf,
    Slime,
    Vex,
    Allay,
    CopperGolem,
    Shulker,
    Wither,
    EnderDragon,
    EndCrystal,
    Goat,
    Sheep,
}

/// Vanilla sheep `getHeadEatPositionScale` and `getHeadEatAngleScale`; `None` keeps the look pitch.
pub(crate) fn sheep_head(eat: f32) -> (f32, Option<f32>) {
    if eat <= 0.0 {
        return (0.0, None);
    }
    let pos = if (4.0..=36.0).contains(&eat) {
        1.0
    } else if eat < 4.0 {
        eat / 4.0
    } else {
        -(eat - 40.0) / 4.0
    };
    let angle = if eat > 4.0 && eat <= 36.0 {
        PI / 5.0 + 0.219_911_49 * ((eat - 4.0) / 32.0 * 28.7).sin()
    } else {
        PI / 5.0
    };
    (pos, Some(angle))
}

pub fn deco_for(name: &str) -> Deco {
    match name {
        "cod" => Deco::Cod,
        "salmon" => Deco::Salmon,
        "tropical_fish" => Deco::TropicalFish,
        "pufferfish" => Deco::Pufferfish,
        "squid" | "glow_squid" => Deco::Squid,
        "phantom" => Deco::Phantom,
        "guardian" | "elder_guardian" => Deco::Guardian,
        "dolphin" => Deco::Dolphin,
        "hoglin" | "zoglin" => Deco::Hoglin,
        "ravager" => Deco::Ravager,
        "turtle" => Deco::Turtle,
        "polar_bear" => Deco::PolarBear,
        "strider" => Deco::Strider,
        "snow_golem" => Deco::SnowGolem,
        "iron_golem" => Deco::IronGolem,
        "mooshroom" => Deco::Mooshroom,
        "enderman" => Deco::Enderman,
        "witch" => Deco::Witch,
        "villager" | "wandering_trader" => Deco::Villager,
        "fox" => Deco::Fox,
        "panda" => Deco::Panda,
        "parrot" => Deco::Parrot,
        "bee" => Deco::Bee,
        "horse" | "skeleton_horse" | "zombie_horse" | "donkey" | "mule" => Deco::Horse,
        "llama" | "trader_llama" => Deco::Llama,
        "wolf" => Deco::Wolf,
        "slime" | "magma_cube" => Deco::Slime,
        "vex" => Deco::Vex,
        "allay" => Deco::Allay,
        "copper_golem" => Deco::CopperGolem,
        "shulker" => Deco::Shulker,
        "wither" => Deco::Wither,
        "ender_dragon" => Deco::EnderDragon,
        "end_crystal" => Deco::EndCrystal,
        "goat" => Deco::Goat,
        "sheep" => Deco::Sheep,
        _ => Deco::None,
    }
}
