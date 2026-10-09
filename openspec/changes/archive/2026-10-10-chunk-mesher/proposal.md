# Proposal

## Why

The renderer core only draws synthetic terrain. Real worlds need a mesher that turns chunk sections into
packed quads with vanilla-accurate culling, lighting, tints and render layers, fast enough to keep up
with chunk streaming.

## What Changes

- New crate `mcr-mesh`: per-state model cache baked once from the client jar, a padded 18^3 mesh input
  snapshotted from `ChunkStore`, a mesher with greedy-merged cube quads, generic model quads, fluids,
  ambient occlusion and biome-blended tints, and a threaded mesh job system.
- `mcr-render`: render layers (opaque and cutout in the main pass, translucent in a sorted later pass),
  multi-size block texture groups with mips, per-section tint tables, vanilla-correct face UV
  orientation, and gamma-space shading to match vanilla colours.
- Quad layouts keep their sizes; reserved bits carry the tint kind and tint cell.

Out of scope: quad-level translucent sorting, animated textures, flowing-fluid UV direction, per-vertex
light interpolation (light is per quad, AO is per corner), block entities, Hi-Z, entity rendering.

## Capabilities

### New Capabilities
- `chunk-meshing`: converting chunk sections into render-layered packed quads with culling, greedy
  merging, ambient occlusion, fluids and biome tints, and scheduling that work off the render thread.

### Modified Capabilities
- `voxel-rendering`: render layers, translucent pass, texture groups and tint tables.

## Impact

New crate `crates/mcr-mesh` (deps: mcr-world, mcr-assets, mcr-data, mcr-render, serde_json). Changes in
`crates/mcr-render` (shaders, renderer, mesh, texture). No change to protocol crates.
