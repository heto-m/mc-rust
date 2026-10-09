mod common;

use mcr_assets::{ModelBaker, TextureArrayBuilder};

#[test]
#[ignore = "needs the real client jar and blocks report; run with --ignored"]
fn bake_every_vanilla_block_state() {
    let rm = common::vanilla();
    let blocks = common::all_block_states();
    let mut baker = ModelBaker::new(&rm);

    let (mut states, mut quads, mut failures) = (0usize, 0usize, Vec::new());
    for block in &blocks {
        for props in &block.states {
            states += 1;
            match baker.bake_block_state(&block.name, &common::as_pairs(props)) {
                Ok(model) => quads += model.quad_count(),
                Err(e) => failures.push(format!("{} {:?}: {e}", block.name, props)),
            }
        }
    }
    let sprites = baker.into_sprites();

    let mut builder = TextureArrayBuilder::default();
    let missing = builder
        .add_from_resources(&rm, &sprites)
        .expect("decode sprites");
    let arrays = builder.build();

    eprintln!(
        "blocks={} states={states} quads={quads} sprites={} missing_sprites={} groups={} failures={}",
        blocks.len(),
        sprites.len(),
        missing.len(),
        arrays.groups.len(),
        failures.len(),
    );
    for id in &missing {
        eprintln!("missing sprite: {}", sprites.get(*id));
    }
    for f in failures.iter().take(30) {
        eprintln!("FAIL {f}");
    }
    assert!(failures.is_empty(), "{} bake failures", failures.len());
    assert!(missing.is_empty(), "{} missing sprites", missing.len());
}

#[test]
#[ignore = "needs the real client jar"]
fn every_blockstate_file_is_parseable() {
    let rm = common::vanilla();
    let mut baker = ModelBaker::new(&rm);
    let files: Vec<_> = rm
        .list_prefix("assets/minecraft/blockstates/")
        .map(str::to_owned)
        .collect();
    assert!(files.len() > 1000);
    for path in &files {
        let name = path
            .strip_prefix("assets/minecraft/blockstates/")
            .and_then(|p| p.strip_suffix(".json"))
            .unwrap();
        baker
            .blockstate(name)
            .unwrap_or_else(|e| panic!("{path}: {e}"));
    }
}
