mod common;

use mcr_assets::{
    DisplayContext, GuiLight, Identifier, ItemBaker, ItemContext, ItemModels, ItemProps, LayerKind,
    PlainItem, ResourceManager,
};

fn vanilla() -> Option<ResourceManager> {
    if !common::jar_path().exists() {
        eprintln!("skipping: no client jar");
        return None;
    }
    Some(common::vanilla())
}

fn id(name: &str) -> Identifier {
    Identifier::parse(name)
}

fn models_of(
    items: &ItemModels,
    item: &str,
    props: &dyn ItemProps,
    ctx: &ItemContext,
) -> Vec<String> {
    items
        .evaluate(&id(item), props, ctx)
        .into_iter()
        .map(|l| match l.kind {
            LayerKind::Model(m) => m.path().to_owned(),
            LayerKind::Special { kind, .. } => format!("special:{}", kind.name()),
            LayerKind::BundleSelectedItem => "bundle_selected".into(),
        })
        .collect()
}

#[derive(Default)]
struct Props {
    dyed: Option<i32>,
    potion: Option<i32>,
    trim: Option<&'static str>,
}

impl ItemProps for Props {
    fn dyed_color(&self) -> Option<i32> {
        self.dyed
    }
    fn potion_color(&self) -> Option<i32> {
        self.potion
    }
    fn trim_material(&self) -> Option<&str> {
        self.trim
    }
}

#[test]
fn every_vanilla_item_definition_evaluates_and_bakes() {
    let Some(rm) = vanilla() else { return };
    let items = ItemModels::load(&rm).expect("parse all items/*.json");
    assert!(items.len() > 1000, "only {} definitions", items.len());

    let ctx = ItemContext::default();
    let mut baker = ItemBaker::new();
    let (mut layers, mut quads, mut specials, mut empty) = (0, 0, 0, 0);
    let mut failures = Vec::new();
    let mut ids: Vec<_> = items.ids().cloned().collect();
    ids.sort();
    for item in &ids {
        let result = items.evaluate(item, &PlainItem, &ctx);
        if result.is_empty() {
            empty += 1;
        }
        for layer in result {
            layers += 1;
            let model = match &layer.kind {
                LayerKind::Model(m) => m.clone(),
                LayerKind::Special { base, .. } => {
                    specials += 1;
                    base.clone()
                }
                LayerKind::BundleSelectedItem => continue,
            };
            match baker.bake(&rm, &model) {
                Ok(b) => quads += b.quads.len(),
                Err(e) => failures.push(format!("{item} -> {model}: {e}")),
            }
        }
    }
    eprintln!(
        "definitions={} layers={layers} specials={specials} empty={empty} quads={quads} failures={}",
        ids.len(),
        failures.len()
    );
    for f in &failures {
        eprintln!("{f}");
    }
    assert!(failures.is_empty(), "{} bake failures", failures.len());
}

#[test]
fn bow_pulling_follows_use_duration() {
    let Some(rm) = vanilla() else { return };
    let items = ItemModels::load(&rm).unwrap();
    let mut ctx = ItemContext::default();
    assert_eq!(models_of(&items, "bow", &PlainItem, &ctx), ["item/bow"]);
    ctx.using_item = true;
    for (ticks, expected) in [
        (0.0, "item/bow_pulling_0"),
        (12.0, "item/bow_pulling_0"),
        (13.0, "item/bow_pulling_1"),
        (18.0, "item/bow_pulling_2"),
    ] {
        ctx.ticks_using_item = ticks;
        assert_eq!(models_of(&items, "bow", &PlainItem, &ctx), [expected]);
    }
}

#[test]
fn compass_and_clock_dispatch_with_defaults() {
    let Some(rm) = vanilla() else { return };
    let items = ItemModels::load(&rm).unwrap();
    let mut ctx = ItemContext::default();
    assert_eq!(
        models_of(&items, "compass", &PlainItem, &ctx),
        ["item/compass_16"]
    );
    assert_eq!(
        models_of(&items, "clock", &PlainItem, &ctx),
        ["item/clock_00"]
    );
    ctx.compass_angle = 5.0 / 32.0;
    assert_eq!(
        models_of(&items, "compass", &PlainItem, &ctx),
        ["item/compass_21"]
    );
    ctx.day_fraction = 0.5;
    assert_eq!(
        models_of(&items, "clock", &PlainItem, &ctx),
        ["item/clock_32"]
    );
    ctx.dimension = "minecraft:the_nether".into();
    let nether = models_of(&items, "clock", &PlainItem, &ctx);
    assert_eq!(nether.len(), 1);
    assert_ne!(nether, ["item/clock_32"]);
}

#[test]
fn potion_and_leather_tints() {
    let Some(rm) = vanilla() else { return };
    let items = ItemModels::load(&rm).unwrap();
    let ctx = ItemContext::default();
    let tint =
        |item: &str, props: &dyn ItemProps| items.evaluate(&id(item), props, &ctx)[0].tints[0];

    assert_eq!(tint("potion", &PlainItem), 0xFF38_5DC6);
    let red = Props {
        potion: Some(0xFF0000),
        ..Props::default()
    };
    assert_eq!(tint("potion", &red), 0xFFFF_0000);

    assert_eq!(tint("leather_helmet", &PlainItem), 0xFFA0_6540);
    let dyed = Props {
        dyed: Some(0x1E_90FF),
        ..Props::default()
    };
    assert_eq!(tint("leather_helmet", &dyed), 0xFF1E_90FF);
}

#[test]
fn trim_material_selects_trimmed_model() {
    let Some(rm) = vanilla() else { return };
    let items = ItemModels::load(&rm).unwrap();
    let ctx = ItemContext::default();
    let trimmed = Props {
        trim: Some("minecraft:quartz"),
        ..Props::default()
    };
    assert_eq!(
        models_of(&items, "leather_helmet", &trimmed, &ctx),
        ["item/leather_helmet_quartz_trim"]
    );
    assert_eq!(
        models_of(&items, "leather_helmet", &PlainItem, &ctx),
        ["item/leather_helmet"]
    );
}

#[test]
fn grass_block_tint_comes_from_the_colormap() {
    let Some(rm) = vanilla() else { return };
    let items = ItemModels::load(&rm).unwrap();
    let layers = items.evaluate(&id("grass_block"), &PlainItem, &ItemContext::default());
    let tint = layers[0].tints[0];
    let (r, g, b) = ((tint >> 16) & 255, (tint >> 8) & 255, tint & 255);
    assert!(g > r && g > b, "{tint:#x}");
}

#[test]
fn chest_is_a_special_marker_with_its_base_model() {
    let Some(rm) = vanilla() else { return };
    let items = ItemModels::load(&rm).unwrap();
    let layers = items.evaluate(&id("chest"), &PlainItem, &ItemContext::default());
    match &layers[0].kind {
        LayerKind::Special { base, kind } => {
            assert_eq!(base.path(), "item/chest");
            assert_eq!(kind.name(), "chest");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn display_transforms_and_generated_extrusion() {
    let Some(rm) = vanilla() else { return };
    let mut baker = ItemBaker::new();
    let cube = baker.bake(&rm, &id("block/cobblestone")).unwrap();
    let gui = cube.display(DisplayContext::Gui).expect("gui transform");
    assert_eq!(gui.rotation, [30.0, 225.0, 0.0]);
    assert_eq!(gui.scale, [0.625; 3]);
    assert_eq!(cube.gui_light, GuiLight::Side);
    assert_eq!(cube.quads.len(), 6);

    let sword = baker.bake(&rm, &id("item/diamond_sword")).unwrap();
    assert_eq!(sword.gui_light, GuiLight::Front);
    assert!(sword.quads.len() > 40, "{}", sword.quads.len());
    assert!(sword.quads.iter().all(|q| q.tint == 0));
}
