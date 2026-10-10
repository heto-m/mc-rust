# Proposal

## Why

The first mesher approximated vanilla: one averaged light value per quad, no random variants or offsets,
no animated textures, still-only fluid tops, section-centre biome tints and full-face-only culling. Side by
side with vanilla at smooth lighting the difference is obvious. Closing these gaps now keeps the quad
formats stable before entities and a world viewer build on them.

## What Changes

- Per-vertex smooth lighting: cube quads grow to 16 bytes and model quads to 40, carrying four corner
  sky/block light pairs (1/16-level resolution, like vanilla's smooth blend) and per-corner ambient
  occlusion. Final brightness is computed in the vertex shader from a 16x16 lightmap uniform built on the
  CPU from an `Environment` (day time, sky darkening, gamma, block tint), so time of day never remeshes.
- Vanilla diagonal and AO term per face (average of four 0.2/1.0 shade terms), bilinear weights for
  partial model faces.
- Random block-state variants chosen with vanilla's `Mth.getSeed` and Java LCG; XZ/XYZ block offsets.
  Cube faces carry a 2-bit UV rotation so rotated variants still merge.
- GPU texture animation: a per-sprite frame table in a storage buffer updated each tick, with
  interpolation, for every animated block sprite.
- Fluids: flow-vector UVs with the flowing sprite, overlay sprite beside glass, backward top face,
  occlusion-shape side/bottom culling, waterlogged blocks, vanilla side/bottom shading.
- Biome tints: per-column biome sampled with vanilla's seeded `BiomeManager` zoom at the tinted block's
  height, blend radius, swamp/dark-forest modifiers, birch/spruce/mangrove constants.
- Partial-shape occlusion: per-face 16x16 occlusion masks give vanilla `Block.shouldRenderFace` between
  slabs, stairs and other partial blocks.
- Distance fog and sky clear colour from the environment.
- **BREAKING** (internal formats only): `PackedQuad`, `CubeQuad`, `ModelQuad`, `ModelQuadDesc` layouts and
  fields change. `SectionMesh`, `MeshPool`, `Renderer::set_section`, `FrameInputs` stay source compatible;
  new behaviour is reached through additive setters.

## Capabilities

### New Capabilities

### Modified Capabilities
- `chunk-meshing`: smooth lighting, variants, offsets, fluids, tints and occlusion fidelity.
- `voxel-rendering`: new quad formats, lightmap, animation table, fog and environment.

## Impact

`crates/mcr-mesh`, `crates/mcr-render`. `mcr-app` is unaffected (public API additive). No new dependencies.
