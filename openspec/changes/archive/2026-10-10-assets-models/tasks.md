## 1. Crate and sources

- [x] 1.1 Scaffold `mcr-assets`, identifiers, `AssetSource` with zip and directory impls, `ResourceManager` layering, pack.mcmeta and overlays; verified by unit tests on fixture packs.

## 2. Models and baking

- [x] 2.1 Model JSON parsing, parent inheritance, texture variable resolution; verified by fixture tests.
- [x] 2.2 Blockstate variants and multipart; verified by fixture tests.
- [x] 2.3 Quad baking with element rotation, blockstate rotation and uvlock; verified by fixture tests and `bake_block_state`.

## 3. Textures

- [x] 3.1 PNG decoding, animation mcmeta, `TextureArrayBuilder`, mip generation; verified by unit tests.

## 4. Integration

- [x] 4.1 `#[ignore]` real-jar test baking every blockstate with zero failures; run locally and record counts.
- [x] 4.2 Criterion bench baking all models from the jar.
- [x] 4.3 `cargo fmt`, clippy `-D warnings`, `cargo test --workspace` pass.
