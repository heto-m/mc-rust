# Tasks

## 1. Extractor

- [x] 1.1 Client classpath script and entry point dumping layers and animations; verify `entity_models.json` and `animations.json` exist with pig head/body/legs.
- [x] 1.2 Renderer scan producing per entity type textures, layers, shadow, scale; verify pig/zombie/creeper entries.

## 2. Crate

- [x] 2.1 JSON loader, cube unwrap, pose evaluator, keyframe sampler; verify with unit tests (pig part counts, cube UVs, keyframe boundary).
- [x] 2.2 Biped/quadruped walk and head look; verify with unit tests.
- [x] 2.3 Instanced skinned wgpu renderer and texture array; verify offscreen PNG of six mobs by eye.

## 3. Wrap-up

- [x] 3.1 fmt, clippy, test pass; archive change.
