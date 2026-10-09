## Why

The renderer needs block geometry and textures, but vanilla stores them as layered JSON models, blockstate definitions and PNG sprites inside the user's own client jar. A GPU-free crate must load those assets (plus resource packs) at runtime and bake them into flat, renderer-ready data.

## What Changes

- Add crate `mcr-assets` (no GPU code).
- Asset sources (zip jar/pack, directory) layered by a `ResourceManager`, with `pack.mcmeta` and overlay handling.
- Blockstate definitions (`variants`, `multipart`) and block model resolution (parents, texture variables, elements, rotations).
- `bake_block_state(name, properties)` producing flat quads with positions, UVs, cull face, tint, shading and sprite references, honouring blockstate x/y/z rotation and uvlock.
- PNG decoding, `.mcmeta` animation, and a `TextureArrayBuilder` that groups sprites by size and generates CPU-side mip chains.
- Synthetic fixture tests, an ignored real-jar integration test, and a criterion bench.

## Capabilities

### New Capabilities
- `asset-loading`: asset sources, resource pack layering, pack metadata.
- `model-baking`: blockstate and block model resolution into baked quads.
- `texture-arrays`: sprite decoding, animation metadata, layered texture arrays with mips.

### Modified Capabilities

## Impact

New crate `crates/mcr-assets`; new workspace-level dependencies (`serde`, `serde_json`, `png`, `zip`, `criterion`). No changes to other crates.
