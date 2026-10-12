mod support;

use mcr_entity_model::mcr_protocol::types::ItemStack;
use mcr_entity_model::mcr_protocol::types::component::{ArmorTrim, DataComponent};
use mcr_entity_model::mcr_protocol::types::entity_data::EntityDataValue as V;
use mcr_entity_model::{Catalog, EntityView, NoData, Plan, PlanCx, TextureCache, default_skin};
use support::*;

fn build(ty: &str, data: &Data, equipment: Option<&[ItemStack; 8]>) -> Option<Plan> {
    let assets = assets()?;
    let catalog = Catalog::new();
    let reg = TestRegistries::vanilla();
    let mut tex = TextureCache::default();
    let mut cx = PlanCx {
        reg: &reg,
        assets: &assets,
        tex: &mut tex,
    };
    let mut view = EntityView::named(ty, &data.0);
    view.equipment = equipment;
    let mut plan = Plan::default();
    catalog.build(&mut cx, &view, &mut plan);
    Some(plan)
}

#[test]
fn every_entity_type_resolves_and_unmapped_ones_use_the_hitbox() {
    let Some(assets) = assets() else { return };
    let catalog = Catalog::new();
    let reg = TestRegistries::vanilla();
    let mut tex = TextureCache::default();
    let version = mcr_data::v26_3();
    let mut unmapped = Vec::new();
    let mut id = 0;
    while let Some(name) = version.entity_type_name(id) {
        let mut cx = PlanCx {
            reg: &reg,
            assets: &assets,
            tex: &mut tex,
        };
        let view = EntityView::new(id as i32, &NoData);
        let mut plan = Plan::default();
        catalog.build(&mut cx, &view, &mut plan);
        for part in &plan.parts {
            assert!(
                part.layer.starts_with("builtin:") || assets.layers.get(&part.layer).is_some(),
                "{name}: missing layer {}",
                part.layer
            );
        }
        if !catalog.is_mapped(id) {
            unmapped.push(name);
            assert_eq!(plan.parts.len(), 1, "{name}");
            assert_eq!(plan.parts[0].layer, "builtin:hitbox");
        }
        id += 1;
    }
    println!("hitbox fallback: {unmapped:?}");
    assert!(unmapped.len() < 12, "{unmapped:?}");
}

#[test]
fn cat_variants_select_textures_and_unknown_ids_fall_back() {
    let tex = |id: i32| {
        build(
            "minecraft:cat",
            &Data::default().with(20, V::CatVariant(id)),
            None,
        )
        .map(|p| p.parts[0].texture.to_string())
    };
    let Some(red) = tex(7) else { return };
    assert_eq!(red, "entity/cat/cat_red");
    assert_eq!(tex(1).unwrap(), "entity/cat/cat_black");
    assert_eq!(tex(9).unwrap(), "entity/cat/cat_tabby");
    assert_eq!(tex(-5).unwrap(), "entity/cat/cat_tabby");
    assert_eq!(tex(i32::MAX).unwrap(), "entity/cat/cat_tabby");
}

#[test]
fn tamed_wolf_gets_a_collar_layer() {
    let data = Data::default().with(18, V::Byte(4)).with(21, V::Int(14));
    let Some(plan) = build("minecraft:wolf", &data, None) else {
        return;
    };
    assert_eq!(plan.parts.len(), 2);
    assert!(plan.parts[1].texture.contains("wolf_collar"));
    assert_eq!(plan.parts[1].tint[..3], [0xB0, 0x2E, 0x26]);
}

#[test]
fn tamed_pets_without_collar_data_wear_the_default_red_collar() {
    let tame = Data::default().with(18, V::Byte(4));
    let Some(wolf) = build("minecraft:wolf", &tame, None) else {
        return;
    };
    assert_eq!(wolf.parts.len(), 2);
    assert_eq!(wolf.parts[1].tint[..3], [0xB0, 0x2E, 0x26]);
    let cat = build("minecraft:cat", &tame, None).unwrap();
    let collar = cat.parts.iter().find(|p| p.texture.contains("cat_collar"));
    assert_eq!(collar.expect("cat collar").tint[..3], [0xB0, 0x2E, 0x26]);
    let wild = build("minecraft:wolf", &Data::default(), None).unwrap();
    assert_eq!(wild.parts.len(), 1);
}

#[test]
fn baby_pig_uses_the_baby_model() {
    let Some(plan) = build(
        "minecraft:pig",
        &Data::default().with(16, V::Boolean(true)),
        None,
    ) else {
        return;
    };
    assert_eq!(plan.parts[0].layer, "minecraft:pig_baby#main");
}

#[test]
fn zombie_armor_and_trim_layers() {
    let mut eq: [ItemStack; 8] = Default::default();
    let mut chest = item("minecraft:diamond_chestplate");
    chest.components.added.push(DataComponent::Trim(ArmorTrim {
        material: mcr_buf::IdOr::Id(5),
        pattern: mcr_buf::IdOr::Id(8),
    }));
    eq[3] = chest;
    let Some(plan) = build("minecraft:zombie", &Data::default(), Some(&eq)) else {
        return;
    };
    let armor: Vec<_> = plan
        .parts
        .iter()
        .filter(|p| p.layer.ends_with("#chestplate"))
        .collect();
    assert_eq!(armor.len(), 2, "armor + trim");
    assert!(armor[0].texture.ends_with("humanoid/diamond"));
    assert!(armor[1].texture.starts_with("dyn:trim:humanoid:sentry:"));
}

#[test]
fn default_skin_follows_uuid_hash() {
    let (name, slim) = default_skin(uuid::Uuid::nil());
    assert_eq!((name, slim), ("alex", true));
    let (_, slim) = default_skin(uuid::Uuid::from_u128(9));
    assert!(!slim, "hash 9 selects the first wide skin");
}

#[test]
fn hostile_entity_data_is_ignored() {
    let data = Data::default()
        .with(18, V::String("x".into()))
        .with(23, V::Boolean(true))
        .with(21, V::Float(f32::NAN));
    for ty in [
        "minecraft:wolf",
        "minecraft:sheep",
        "minecraft:villager",
        "minecraft:horse",
        "minecraft:tropical_fish",
    ] {
        assert!(build(ty, &data, None).is_none_or(|p| !p.parts.is_empty()));
    }
}

fn build_with(ty: &str, data: &Data, set: impl FnOnce(&mut EntityView<'_>)) -> Option<Plan> {
    let assets = assets()?;
    let catalog = Catalog::new();
    let reg = TestRegistries::vanilla();
    let mut tex = TextureCache::default();
    let mut cx = PlanCx {
        reg: &reg,
        assets: &assets,
        tex: &mut tex,
    };
    let mut view = EntityView::named(ty, &data.0);
    set(&mut view);
    let mut plan = Plan::default();
    catalog.build(&mut cx, &view, &mut plan);
    Some(plan)
}

#[test]
fn grazing_sheep_lowers_its_head() {
    let data = Data::default();
    let Some(idle) = build_with("minecraft:sheep", &data, |_| {}) else {
        return;
    };
    assert_eq!(idle.anim.head_x, None);
    assert_eq!(idle.anim.head_dy, 0.0);
    let eating = build_with("minecraft:sheep", &data, |v| v.events.eat = 20.0).unwrap();
    assert_eq!(eating.anim.head_dy, 9.0);
    assert!(eating.anim.head_x.unwrap() > 0.4);
    let start = build_with("minecraft:sheep", &data, |v| v.events.eat = 38.0).unwrap();
    assert!((start.anim.head_dy - 4.5).abs() < 1e-4);
}

#[test]
fn ramming_goat_pitches_its_head_down() {
    let data = Data::default();
    let Some(goat) = build_with("minecraft:goat", &data, |v| v.events.ram = 20.0) else {
        return;
    };
    assert!((goat.anim.head_x.unwrap() - 30f32.to_radians()).abs() < 1e-5);
}

#[test]
fn evoker_fangs_stay_hidden_until_they_bite() {
    let data = Data::default();
    let Some(idle) = build_with("minecraft:evoker_fangs", &data, |_| {}) else {
        return;
    };
    assert!(idle.parts.is_empty());
    let biting = build_with("minecraft:evoker_fangs", &data, |v| v.events.bite = 0.3).unwrap();
    assert_eq!(biting.parts.len(), 1);
    assert_eq!(biting.anim.bite, 0.3);
    assert_eq!(biting.body_yaw_offset, 90.0);
}

#[test]
fn hit_armor_stand_wiggles_for_five_ticks() {
    let data = Data::default();
    let Some(hit) = build_with("minecraft:armor_stand", &data, |v| {
        v.events.wiggle = Some(0.75)
    }) else {
        return;
    };
    assert!((hit.body_yaw_offset + 3.0).abs() < 1e-4);
    let later = build_with("minecraft:armor_stand", &data, |v| {
        v.events.wiggle = Some(6.0)
    })
    .unwrap();
    assert_eq!(later.body_yaw_offset, 0.0);
}

#[test]
fn rabbits_hop_only_after_a_jump_event() {
    let data = Data::default();
    let Some(idle) = build_with("minecraft:rabbit", &data, |v| v.limb_amount = 1.0) else {
        return;
    };
    assert!(idle.keyframes.is_empty());
    let hop = build_with("minecraft:rabbit", &data, |v| v.events.hop = Some(4.0)).unwrap();
    assert_eq!(hop.keyframes, vec![("RabbitAnimation.HOP", 0.2, 1.0)]);
}

#[test]
fn warden_event_animations_play_once() {
    let data = Data::default();
    let Some(attack) = build_with("minecraft:warden", &data, |v| {
        v.events.attack = Some(2.0);
        v.events.tendril = 1.0;
    }) else {
        return;
    };
    assert!(
        attack
            .keyframes
            .iter()
            .any(|k| k.0 == "WardenAnimation.WARDEN_ATTACK")
    );
    assert_eq!(attack.rot_overrides.len(), 2);
    let done = build_with("minecraft:warden", &data, |v| v.events.attack = Some(40.0)).unwrap();
    assert!(done.keyframes.is_empty());
    assert!(done.rot_overrides.is_empty());
}

#[test]
fn stunned_ravager_and_attacking_hoglin_reach_the_rig() {
    let data = Data::default();
    let Some(ravager) = build_with("minecraft:ravager", &data, |v| {
        v.events.stunned = 30.0;
        v.attack_ticks = 4.0;
    }) else {
        return;
    };
    assert_eq!(ravager.anim.stunned, 30.0);
    assert_eq!(ravager.anim.attack_ticks, 4.0);
    let hoglin = build_with("minecraft:hoglin", &data, |v| v.attack_ticks = 7.0).unwrap();
    assert_eq!(hoglin.anim.attack_ticks, 7.0);
}

fn elytra_player(flags: i8, set: impl FnOnce(&mut EntityView<'_>)) -> Option<Plan> {
    let mut eq: [ItemStack; 8] = Default::default();
    eq[3] = item("minecraft:elytra");
    let data = Data::default().with(0, V::Byte(flags));
    build_with("minecraft:player", &data, |v| {
        v.equipment = Some(Box::leak(Box::new(eq)));
        set(v);
    })
}

fn wing(plan: &Plan, left: bool) -> [f32; 3] {
    use mcr_entity_model::Slot;
    let slot = if left {
        Slot::LeftWing
    } else {
        Slot::RightWing
    };
    plan.rot_overrides
        .iter()
        .find(|(s, _)| *s == slot)
        .map(|(_, r)| *r)
        .expect("wing rotation")
}

#[test]
fn gliding_player_lies_along_its_pitch_and_rolls_into_the_turn() {
    use mcr_entity_model::glam::Vec3;
    let Some(standing) = elytra_player(0, |_| {}) else {
        return;
    };
    assert_eq!(standing.post_rot, mcr_entity_model::glam::Mat4::IDENTITY);
    let plan = elytra_player(0x80u8 as i8, |v| {
        v.pitch = 0.0;
        v.glide.ticks = 10.0;
    })
    .unwrap();
    let up = plan.post_rot.transform_vector3(Vec3::Y);
    assert!((up - Vec3::NEG_Z).length() < 1e-5, "{up}");
    let half = elytra_player(0x80u8 as i8, |v| {
        v.pitch = -30.0;
        v.glide.ticks = 5.0;
        v.glide.y_rot = Some(0.5);
    })
    .unwrap();
    let expected = mcr_entity_model::glam::Mat4::from_rotation_x((0.25f32 * -60.0).to_radians())
        * mcr_entity_model::glam::Mat4::from_rotation_y(0.5);
    assert!(half.post_rot.abs_diff_eq(expected, 1e-5));
}

#[test]
fn elytra_wings_mirror_the_animation_state_and_drop_when_crouching() {
    use mcr_entity_model::mcr_protocol::types::entity_data::Pose;
    let Some(plan) = elytra_player(0, |v| v.glide.wings = [0.3, 0.1, -1.2]) else {
        return;
    };
    let deg = |r: f32| r.to_degrees();
    assert_eq!(wing(&plan, true), [deg(0.3), deg(0.1), deg(-1.2)]);
    assert_eq!(wing(&plan, false), [deg(0.3), -deg(0.1), deg(1.2)]);
    let elytra = plan
        .parts
        .iter()
        .find(|p| p.layer == "minecraft:elytra#main")
        .expect("elytra part");
    assert_eq!(elytra.offset.y, 0.0);
    let data = Data::default().with(6, V::Pose(Pose::Crouching));
    let mut eq: [ItemStack; 8] = Default::default();
    eq[3] = item("minecraft:elytra");
    let crouched = build("minecraft:player", &data, Some(&eq)).unwrap();
    let elytra = crouched
        .parts
        .iter()
        .find(|p| p.layer == "minecraft:elytra#main")
        .unwrap();
    assert_eq!(elytra.offset.y, 3.0);
}

#[test]
fn zombies_and_armor_stands_wear_elytra_too() {
    let mut eq: [ItemStack; 8] = Default::default();
    eq[3] = item("minecraft:elytra");
    for (ty, layer) in [
        ("minecraft:zombie", "minecraft:elytra#main"),
        ("minecraft:armor_stand", "minecraft:elytra#main"),
    ] {
        let Some(plan) = build(ty, &Data::default(), Some(&eq)) else {
            return;
        };
        assert!(plan.parts.iter().any(|p| p.layer == layer), "{ty}");
    }
}

#[test]
fn thrown_items_are_single_camera_facing_items() {
    use mcr_entity_model::glam::{Mat4, Vec3};
    let data = Data::default().with(8, V::ItemStack(item("minecraft:snowball")));
    let Some(snowball) = build("minecraft:snowball", &data, None) else {
        return;
    };
    assert_eq!(snowball.free_items.len(), 1);
    let it = &snowball.free_items[0];
    assert!(it.billboard && !it.ground_lift);
    assert_eq!(it.local, Mat4::IDENTITY, "no lift, bob or spin");
    assert!(!snowball.bright);
    let data = Data::default().with(8, V::ItemStack(item("minecraft:fire_charge")));
    let fireball = build("minecraft:fireball", &data, None).unwrap();
    assert_eq!(
        fireball.free_items[0].local,
        Mat4::from_scale(Vec3::splat(3.0))
    );
    assert!(fireball.bright);
    let small = build("minecraft:small_fireball", &data, None).unwrap();
    assert_eq!(
        small.free_items[0].local,
        Mat4::from_scale(Vec3::splat(0.75))
    );
    let dragon = build("minecraft:dragon_fireball", &Data::default(), None).unwrap();
    assert!(dragon.free_items.is_empty() && dragon.free_geo.len() == 1 && dragon.bright);
}

#[test]
fn primed_tnt_swells_and_flashes_from_its_fuse() {
    use mcr_entity_model::FreeGeo;
    let data = Data::default();
    let block_scale = |p: &Plan| match &p.free_geo[0] {
        FreeGeo::Block { local, .. } => local.x_axis.x,
        _ => panic!("tnt is a block"),
    };
    let Some(idle) = build_with("minecraft:tnt", &data, |v| v.fuse = Some(35.5)) else {
        return;
    };
    assert_eq!(block_scale(&idle), 1.0);
    assert!(!idle.flash, "fuse 35.5 is in an odd window");
    let late = build_with("minecraft:tnt", &data, |v| v.fuse = Some(2.0)).unwrap();
    assert!((block_scale(&late) - (1.0 + 0.8f32.powi(4) * 0.3)).abs() < 1e-5);
    assert!(late.flash);
    let from_data = build("minecraft:tnt", &Data::default().with(8, V::Int(3)), None).unwrap();
    assert!(from_data.flash && block_scale(&from_data) > 1.0);
}

#[test]
fn minecarts_carry_their_display_block_and_tnt_carts_flash() {
    use mcr_entity_model::FreeGeo;
    let Some(cart) = build("minecraft:minecart", &Data::default(), None) else {
        return;
    };
    assert!(cart.free_geo.is_empty());
    let tnt = build("minecraft:tnt_minecart", &Data::default(), None).unwrap();
    assert!(matches!(tnt.free_geo[..], [FreeGeo::Block { .. }]) && !tnt.flash);
    let lit = build_with("minecraft:tnt_minecart", &Data::default(), |v| {
        v.fuse = Some(3.0)
    })
    .unwrap();
    assert!(lit.flash);
    let custom = Data::default().with(11, V::OptionalBlockState(Some(1)));
    let plan = build("minecraft:minecart", &custom, None).unwrap();
    assert!(matches!(
        plan.free_geo[..],
        [FreeGeo::Block { state: 1, .. }]
    ));
}

#[test]
fn creeper_flashes_white_on_alternate_tenths_of_the_swell() {
    let Some(assets) = assets() else { return };
    let catalog = Catalog::new();
    let reg = TestRegistries::vanilla();
    let mut tex = TextureCache::default();
    let data = Data::default();
    let mut plan_at = |swelling: f32| {
        let mut cx = PlanCx {
            reg: &reg,
            assets: &assets,
            tex: &mut tex,
        };
        let mut view = EntityView::named("minecraft:creeper", &data.0);
        view.swelling = swelling;
        let mut plan = Plan::default();
        catalog.build(&mut cx, &view, &mut plan);
        plan
    };
    assert!(!plan_at(0.0).flash_parts);
    assert!(!plan_at(0.05).flash_parts);
    assert!(plan_at(0.15).flash_parts);
    assert!(!plan_at(0.25).flash_parts);
    let full = plan_at(0.95);
    assert!(full.flash_parts && full.extra_scale.x > 1.3 && full.extra_scale.y > 1.05);
}
