use crate::testutil::{solid_png, zip_bytes};
use crate::*;

fn manager(files: &[(&str, &str)]) -> ResourceManager {
    let owned: Vec<(String, Vec<u8>)> = files
        .iter()
        .map(|(p, c)| (format!("assets/minecraft/{p}"), c.as_bytes().to_vec()))
        .collect();
    let refs: Vec<(&str, &[u8])> = owned
        .iter()
        .map(|(p, c)| (p.as_str(), c.as_slice()))
        .collect();
    let mut rm = ResourceManager::new();
    rm.push_pack(
        "fixture",
        Box::new(ZipSource::from_bytes(zip_bytes(&refs)).unwrap()),
    )
    .unwrap();
    rm
}

const CUBE: &str = r##"{"elements":[{"from":[0,0,0],"to":[16,16,16],"faces":{
    "down":{"texture":"#down","cullface":"down"},"up":{"texture":"#up","cullface":"up"},
    "north":{"texture":"#side","cullface":"north"},"south":{"texture":"#side","cullface":"south"},
    "west":{"texture":"#side","cullface":"west"},"east":{"texture":"#side","cullface":"east"}}}]}"##;

fn sprite_name(baker: &ModelBaker, quad: &BakedQuad) -> String {
    baker.sprites().get(quad.sprite).to_string()
}

fn approx(a: [f32; 3], b: [f32; 3]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5)
}

#[test]
fn inheritance_and_texture_variable_chains() {
    let rm = manager(&[
        ("models/block/cube.json", CUBE),
        (
            "models/block/column.json",
            r##"{"parent":"block/cube","ambientocclusion":false,"textures":{"down":"#end","up":"#end","side":"#side"}}"##,
        ),
        (
            "models/block/log.json",
            r##"{"parent":"block/column","textures":{"end":"block/log_top","side":"minecraft:block/log"}}"##,
        ),
        (
            "blockstates/log.json",
            r#"{"variants":{"":{"model":"minecraft:block/log"}}}"#,
        ),
    ]);
    let mut baker = ModelBaker::new(&rm);
    let model = baker.bake_block_state("minecraft:log", &[]).unwrap();
    let variant = &model.parts[0].variants[0];
    assert_eq!(variant.quads.len(), 6);
    assert!(!variant.ambient_occlusion);
    let names: Vec<_> = variant
        .quads
        .iter()
        .map(|q| sprite_name(&baker, q))
        .collect();
    assert_eq!(names[0], "minecraft:block/log_top");
    assert_eq!(names[1], "minecraft:block/log_top");
    assert_eq!(names[2], "minecraft:block/log");
}

#[test]
fn unresolved_variable_is_an_error() {
    let rm = manager(&[
        ("models/block/cube.json", CUBE),
        (
            "models/block/bad.json",
            r#"{"parent":"block/cube","textures":{"up":"x/y"}}"#,
        ),
        (
            "blockstates/bad.json",
            r#"{"variants":{"":{"model":"block/bad"}}}"#,
        ),
    ]);
    let err = ModelBaker::new(&rm)
        .bake_block_state("bad", &[])
        .unwrap_err();
    assert!(matches!(err, AssetError::UnresolvedTexture { .. }), "{err}");
}

#[test]
fn parent_cycle_is_detected() {
    let rm = manager(&[
        ("models/block/a.json", r#"{"parent":"block/b"}"#),
        ("models/block/b.json", r#"{"parent":"block/a"}"#),
        (
            "blockstates/a.json",
            r#"{"variants":{"":{"model":"block/a"}}}"#,
        ),
    ]);
    let err = ModelBaker::new(&rm).bake_block_state("a", &[]).unwrap_err();
    assert!(matches!(err, AssetError::ParentCycle(_)), "{err}");
}

#[test]
fn default_uvs_and_vertex_order() {
    let rm = manager(&[
        (
            "models/block/slab.json",
            r##"{"textures":{"t":"block/t"},"elements":[{"from":[0,0,0],"to":[16,8,16],"faces":{
                "up":{"texture":"#t"},"south":{"texture":"#t","cullface":"south","tintindex":0}}}]}"##,
        ),
        (
            "blockstates/slab.json",
            r#"{"variants":{"":{"model":"block/slab"}}}"#,
        ),
    ]);
    let mut baker = ModelBaker::new(&rm);
    let q = &baker.bake_block_state("slab", &[]).unwrap().parts[0].variants[0].quads;
    let up = q.iter().find(|q| q.normal == Face::Up).unwrap();
    assert!(approx(up.vertices[0].pos, [0.0, 0.5, 0.0]));
    assert!(approx(up.vertices[2].pos, [1.0, 0.5, 1.0]));
    assert_eq!(up.vertices[0].uv, [0.0, 0.0]);
    assert_eq!(up.vertices[2].uv, [1.0, 1.0]);
    let south = q.iter().find(|q| q.normal == Face::South).unwrap();
    assert_eq!(south.cull, Some(Face::South));
    assert_eq!(south.tint, 0);
    assert_eq!(south.vertices[0].uv, [0.0, 0.5]);
    assert_eq!(south.vertices[1].uv, [0.0, 1.0]);
}

#[test]
fn face_uv_rotation_shifts_corners() {
    let rm = manager(&[
        (
            "models/block/r.json",
            r##"{"textures":{"t":"block/t"},"elements":[{"from":[0,0,0],"to":[16,16,16],"faces":{
                "up":{"texture":"#t","uv":[0,0,16,8],"rotation":90}}}]}"##,
        ),
        (
            "blockstates/r.json",
            r#"{"variants":{"":{"model":"block/r"}}}"#,
        ),
    ]);
    let q = &ModelBaker::new(&rm)
        .bake_block_state("r", &[])
        .unwrap()
        .parts[0]
        .variants[0]
        .quads[0];
    assert_eq!(q.vertices[0].uv, [0.0, 0.5]);
    assert_eq!(q.vertices[1].uv, [1.0, 0.5]);
    assert_eq!(q.vertices[2].uv, [1.0, 0.0]);
}

#[test]
fn y_rotation_moves_geometry_and_cull() {
    let rm = manager(&[
        (
            "models/block/half.json",
            r##"{"textures":{"t":"block/t"},"elements":[{"from":[0,0,0],"to":[8,16,16],"faces":{
                "north":{"texture":"#t","cullface":"north"},"west":{"texture":"#t","cullface":"west"}}}]}"##,
        ),
        (
            "blockstates/half.json",
            r#"{"variants":{"f=n":{"model":"block/half"},"f=e":{"model":"block/half","y":90}}}"#,
        ),
    ]);
    let mut baker = ModelBaker::new(&rm);
    let plain = baker.bake_block_state("half", &[("f", "n")]).unwrap();
    let rotated = baker.bake_block_state("half", &[("f", "e")]).unwrap();
    let a = &plain.parts[0].variants[0].quads;
    let b = &rotated.parts[0].variants[0].quads;
    assert_eq!((a[0].normal, a[0].cull), (Face::North, Some(Face::North)));
    assert_eq!((b[0].normal, b[0].cull), (Face::East, Some(Face::East)));
    assert_eq!((b[1].normal, b[1].cull), (Face::North, Some(Face::North)));
    for v in b[0].vertices {
        assert!(
            (v.pos[0] - 1.0).abs() < 1e-6,
            "east face sits at x=1: {:?}",
            v.pos
        );
        assert!(v.pos[2] <= 0.5 + 1e-6);
    }
}

#[test]
fn x_rotation_turns_top_to_north() {
    let rm = manager(&[
        (
            "models/block/t.json",
            r##"{"textures":{"t":"block/t"},"elements":[{"from":[0,0,0],"to":[16,16,16],"faces":{"up":{"texture":"#t","cullface":"up"}}}]}"##,
        ),
        (
            "blockstates/t.json",
            r#"{"variants":{"":{"model":"block/t","x":90}}}"#,
        ),
    ]);
    let q = &ModelBaker::new(&rm)
        .bake_block_state("t", &[])
        .unwrap()
        .parts[0]
        .variants[0]
        .quads[0];
    assert_eq!((q.normal, q.cull), (Face::North, Some(Face::North)));
}

#[test]
fn uvlock_keeps_texture_world_aligned() {
    let rm = manager(&[
        (
            "models/block/top.json",
            r##"{"textures":{"t":"block/t"},"elements":[{"from":[0,0,0],"to":[16,16,16],"faces":{"up":{"texture":"#t"}}}]}"##,
        ),
        (
            "blockstates/top.json",
            r#"{"variants":{"lock=true":{"model":"block/top","y":90,"uvlock":true},
                            "lock=false":{"model":"block/top","y":90}}}"#,
        ),
    ]);
    let mut baker = ModelBaker::new(&rm);
    let locked = &baker
        .bake_block_state("top", &[("lock", "true")])
        .unwrap()
        .parts[0]
        .variants[0]
        .quads[0];
    for v in locked.vertices {
        assert!(
            (v.uv[0] - v.pos[0]).abs() < 1e-5 && (v.uv[1] - v.pos[2]).abs() < 1e-5,
            "{v:?}"
        );
    }
    let free = &baker
        .bake_block_state("top", &[("lock", "false")])
        .unwrap()
        .parts[0]
        .variants[0]
        .quads[0];
    assert!(
        free.vertices
            .iter()
            .any(|v| (v.uv[0] - v.pos[0]).abs() > 0.5)
    );
}

#[test]
fn uvlock_on_side_face_with_x_rotation() {
    let rm = manager(&[
        (
            "models/block/s.json",
            r##"{"textures":{"t":"block/t"},"elements":[{"from":[0,0,0],"to":[16,16,16],"faces":{"south":{"texture":"#t"}}}]}"##,
        ),
        (
            "blockstates/s.json",
            r#"{"variants":{"":{"model":"block/s","x":180,"uvlock":true}}}"#,
        ),
    ]);
    let q = &ModelBaker::new(&rm)
        .bake_block_state("s", &[])
        .unwrap()
        .parts[0]
        .variants[0]
        .quads[0];
    assert_eq!(q.normal, Face::North);
    for v in q.vertices {
        assert!(
            (v.uv[0] - (1.0 - v.pos[0])).abs() < 1e-5 && (v.uv[1] - (1.0 - v.pos[1])).abs() < 1e-5,
            "{v:?}"
        );
    }
}

#[test]
fn element_rotation_axis_euler_and_rescale() {
    let rm = manager(&[
        (
            "models/block/cross.json",
            r##"{"textures":{"t":"block/t"},"elements":[{"from":[0.8,0,8],"to":[15.2,16,8],
                "rotation":{"origin":[8,8,8],"axis":"y","angle":45,"rescale":true},
                "shade_direction_override":"up","light_emission":7,
                "faces":{"north":{"texture":"#t","uv":[0,0,16,16]},"south":{"texture":"#t","uv":[0,0,16,16]}}}]}"##,
        ),
        (
            "models/block/euler.json",
            r##"{"textures":{"t":"block/t"},"elements":[{"from":[0,0,8],"to":[16,16,8],
                "rotation":{"origin":[8,8,8],"x":0,"y":45,"z":0,"rescale":true},
                "faces":{"north":{"texture":"#t"}}}]}"##,
        ),
        (
            "blockstates/cross.json",
            r#"{"variants":{"":{"model":"block/cross"}}}"#,
        ),
        (
            "blockstates/euler.json",
            r#"{"variants":{"":{"model":"block/euler"}}}"#,
        ),
    ]);
    let mut baker = ModelBaker::new(&rm);
    let cross = &baker.bake_block_state("cross", &[]).unwrap().parts[0].variants[0].quads;
    assert_eq!(cross.len(), 2);
    assert_eq!(cross[0].shade_dir, Face::Up);
    assert_eq!(cross[0].light_emission, 7);
    let xs: Vec<f32> = cross[0].vertices.iter().map(|v| v.pos[0]).collect();
    let min = xs.iter().copied().fold(f32::MAX, f32::min);
    let max = xs.iter().copied().fold(f32::MIN, f32::max);
    assert!(min > -0.01 && max < 1.01 && (max - min) > 0.7, "{xs:?}");
    let euler = &baker.bake_block_state("euler", &[]).unwrap().parts[0].variants[0].quads[0];
    let ys: Vec<f32> = euler.vertices.iter().map(|v| v.pos[1]).collect();
    assert!(ys.contains(&0.0) && ys.contains(&1.0));
    let ex: Vec<f32> = euler.vertices.iter().map(|v| v.pos[0]).collect();
    let span =
        ex.iter().copied().fold(f32::MIN, f32::max) - ex.iter().copied().fold(f32::MAX, f32::min);
    assert!((span - 1.0).abs() < 1e-4, "{span}");
}

#[test]
fn multipart_and_weighted_variants() {
    let rm = manager(&[
        (
            "models/block/post.json",
            r##"{"textures":{"t":"block/t"},"elements":[{"from":[6,0,6],"to":[10,16,10],"faces":{"up":{"texture":"#t"}}}]}"##,
        ),
        (
            "models/block/side.json",
            r##"{"textures":{"t":"block/t"},"elements":[{"from":[7,0,0],"to":[9,16,6],"faces":{"up":{"texture":"#t"}}}]}"##,
        ),
        (
            "blockstates/fence.json",
            r#"{"multipart":[{"apply":{"model":"block/post"}},
                {"when":{"north":"true"},"apply":{"model":"block/side"}},
                {"when":{"east":"true"},"apply":{"model":"block/side","y":90,"uvlock":true}}]}"#,
        ),
        (
            "blockstates/rand.json",
            r#"{"variants":{"":[{"model":"block/post","weight":3},{"model":"block/side","y":180}]}}"#,
        ),
    ]);
    let mut baker = ModelBaker::new(&rm);
    let none = baker
        .bake_block_state("fence", &[("north", "false"), ("east", "false")])
        .unwrap();
    assert_eq!(none.parts.len(), 1);
    let both = baker
        .bake_block_state("fence", &[("north", "true"), ("east", "true")])
        .unwrap();
    assert_eq!(both.parts.len(), 3);
    assert_eq!(both.quad_count(), 3);
    let rand = baker.bake_block_state("rand", &[]).unwrap();
    let part = &rand.parts[0];
    assert_eq!((part.variants.len(), part.total_weight), (2, 4));
    assert_eq!(part.pick(0).weight, 3);
    assert_eq!(part.pick(2).weight, 3);
    assert_eq!(part.pick(3).weight, 1);
}

#[test]
fn later_pack_overrides_models() {
    let mut rm = manager(&[
        (
            "models/block/m.json",
            r##"{"textures":{"old":"block/old"},"elements":[{"from":[0,0,0],"to":[16,16,16],"faces":{"up":{"texture":"#old"}}}]}"##,
        ),
        (
            "blockstates/m.json",
            r#"{"variants":{"":{"model":"block/m"}}}"#,
        ),
    ]);
    let over = zip_bytes(&[(
        "assets/minecraft/models/block/m.json",
        br##"{"textures":{"new":"block/new"},"elements":[{"from":[0,0,0],"to":[16,16,16],"faces":{"up":{"texture":"#new"}}}]}"##,
    )]);
    rm.push_pack("over", Box::new(ZipSource::from_bytes(over).unwrap()))
        .unwrap();
    let mut baker = ModelBaker::new(&rm);
    let q = baker.bake_block_state("m", &[]).unwrap().parts[0].variants[0].quads[0];
    assert_eq!(sprite_name(&baker, &q), "minecraft:block/new");
}

#[test]
fn textures_flow_into_arrays() {
    let rm = manager(&[
        ("models/block/cube.json", CUBE),
        (
            "models/block/c.json",
            r##"{"parent":"block/cube","textures":{"down":"block/a","up":"block/a","side":"block/b"}}"##,
        ),
        (
            "blockstates/c.json",
            r#"{"variants":{"":{"model":"block/c"}}}"#,
        ),
    ]);
    let mut baker = ModelBaker::new(&rm);
    baker.bake_block_state("c", &[]).unwrap();
    let sprites = baker.into_sprites();
    assert_eq!(sprites.len(), 2);

    let mut files = vec![(
        "assets/minecraft/textures/block/a.png",
        solid_png(16, 16, [1, 2, 3, 255]),
    )];
    files.push((
        "assets/minecraft/textures/block/b.png",
        solid_png(32, 32, [1, 2, 3, 255]),
    ));
    let refs: Vec<(&str, &[u8])> = files.iter().map(|(p, b)| (*p, b.as_slice())).collect();
    let mut tex_rm = ResourceManager::new();
    tex_rm
        .push_pack(
            "tex",
            Box::new(ZipSource::from_bytes(zip_bytes(&refs)).unwrap()),
        )
        .unwrap();
    let mut builder = TextureArrayBuilder::default();
    let missing = builder.add_from_resources(&tex_rm, &sprites).unwrap();
    assert!(missing.is_empty());
    let arrays = builder.build();
    assert_eq!(arrays.groups.len(), 2);
}
