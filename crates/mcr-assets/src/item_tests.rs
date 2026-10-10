use crate::testutil::{png_bytes, zip_bytes};
use crate::*;

fn manager(files: &[(&str, &[u8])]) -> ResourceManager {
    let owned: Vec<(String, &[u8])> = files
        .iter()
        .map(|(p, c)| (format!("assets/minecraft/{p}"), *c))
        .collect();
    let refs: Vec<(&str, &[u8])> = owned.iter().map(|(p, c)| (p.as_str(), *c)).collect();
    let mut rm = ResourceManager::new();
    rm.push_pack(
        "fixture",
        Box::new(ZipSource::from_bytes(zip_bytes(&refs)).unwrap()),
    )
    .unwrap();
    rm
}

const GENERATED: &[u8] = br#"{"parent":"item/generated","textures":{"layer0":"item/dot"}}"#;
const PARENT: &[u8] = br#"{"parent":"builtin/generated","gui_light":"front","display":{"gui":{"scale":[2,2,2]},"ground":{"translation":[0,32,0]}}}"#;

#[test]
fn single_opaque_pixel_has_four_edges_plus_front_and_back() {
    let mut pixels = vec![0u8; 3 * 3 * 4];
    pixels[(4 * 4)..(4 * 4 + 4)].copy_from_slice(&[255, 0, 0, 255]);
    let png = png_bytes(3, 3, &pixels);
    let rm = manager(&[
        ("models/item/generated.json", PARENT),
        ("models/item/dot.json", GENERATED),
        ("textures/item/dot.png", &png),
    ]);
    let mut baker = ItemBaker::new();
    let model = baker
        .bake(&rm, &Identifier::parse("minecraft:item/dot"))
        .unwrap();
    assert_eq!(model.quads.len(), 6);
    assert_eq!(model.builtin, Some(Builtin::Generated));
    assert_eq!(model.gui_light, GuiLight::Front);
    let normals: Vec<_> = model.quads.iter().map(|q| q.normal).collect();
    for face in [
        Face::South,
        Face::North,
        Face::Up,
        Face::Down,
        Face::East,
        Face::West,
    ] {
        assert!(normals.contains(&face), "{face:?} missing in {normals:?}");
    }
}

#[test]
fn display_transforms_inherit_per_context_and_clamp() {
    let png = png_bytes(1, 1, &[1, 2, 3, 255]);
    let rm = manager(&[
        ("models/item/generated.json", PARENT),
        (
            "models/item/child.json",
            br#"{"parent":"item/dot","display":{"gui":{"rotation":[1,2,3]}}}"#,
        ),
        ("models/item/dot.json", GENERATED),
        ("textures/item/dot.png", &png),
    ]);
    let mut baker = ItemBaker::new();
    let model = baker
        .bake(&rm, &Identifier::parse("minecraft:item/child"))
        .unwrap();
    let gui = model.display(DisplayContext::Gui).unwrap();
    assert_eq!(gui.rotation, [1.0, 2.0, 3.0]);
    assert_eq!(gui.scale, [1.0; 3]);
    let ground = model.display(DisplayContext::Ground).unwrap();
    assert_eq!(ground.translation, [0.0, 2.0, 0.0]);
    assert!(model.display(DisplayContext::Head).is_none());
}

#[test]
fn select_range_and_condition_nodes_evaluate() {
    let def = br##"{"model":{"type":"minecraft:condition","property":"minecraft:using_item",
        "on_false":{"type":"minecraft:model","model":"item/a"},
        "on_true":{"type":"minecraft:range_dispatch","property":"minecraft:use_duration","scale":0.5,
            "entries":[{"threshold":2,"model":{"type":"minecraft:model","model":"item/c"}},
                       {"threshold":1,"model":{"type":"minecraft:model","model":"item/b"}}],
            "fallback":{"type":"minecraft:model","model":"item/a"}}}}"##;
    let select = br##"{"model":{"type":"minecraft:select","property":"minecraft:display_context",
        "cases":[{"when":["gui","fixed"],"model":{"type":"minecraft:model","model":"item/a"}}],
        "fallback":{"type":"minecraft:empty"}}}"##;
    let rm = manager(&[("items/t.json", def), ("items/s.json", select)]);
    let items = ItemModels::load(&rm).unwrap();
    let mut ctx = ItemContext::default();
    let eval = |items: &ItemModels, name: &str, ctx: &ItemContext| -> Vec<String> {
        items
            .evaluate(&Identifier::parse(name), &PlainItem, ctx)
            .into_iter()
            .filter_map(|l| match l.kind {
                LayerKind::Model(m) => Some(m.path().to_owned()),
                _ => None,
            })
            .collect()
    };
    assert_eq!(eval(&items, "t", &ctx), ["item/a"]);
    ctx.using_item = true;
    ctx.ticks_using_item = 1.0;
    assert_eq!(eval(&items, "t", &ctx), ["item/a"]);
    ctx.ticks_using_item = 2.0;
    assert_eq!(eval(&items, "t", &ctx), ["item/b"]);
    ctx.ticks_using_item = 4.0;
    assert_eq!(eval(&items, "t", &ctx), ["item/c"]);

    assert_eq!(eval(&items, "s", &ctx), ["item/a"]);
    ctx.display = DisplayContext::Ground;
    assert!(eval(&items, "s", &ctx).is_empty());
}

#[test]
fn unknown_node_type_is_an_error_naming_the_file() {
    let rm = manager(&[(
        "items/bad.json",
        br#"{"model":{"type":"minecraft:nonsense"}}"#,
    )]);
    let err = ItemModels::load(&rm).unwrap_err().to_string();
    assert!(
        err.contains("items/bad.json") && err.contains("nonsense"),
        "{err}"
    );
}
