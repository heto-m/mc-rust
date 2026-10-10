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
