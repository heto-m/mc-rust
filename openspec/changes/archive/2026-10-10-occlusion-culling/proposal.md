# Proposal

## Why

At render distance 32 the renderer is GPU-bound: about 500 fps looking at the horizon against 2000+ looking
up or down. Every frustum-visible section is drawn, including caves and terrain hidden behind hills, because
culling stops at the frustum and per-direction back faces; the Hi-Z hook was a stub.

## What Changes

- Sodium/vanilla-style section visibility graph: the mesher records which faces of a section connect through
  non-opaque cells; the renderer searches from the camera section, only moving away from the camera through
  connected faces, and draws only reached sections. Skipped while the camera is inside an opaque block.
- Two-phase Hi-Z occlusion culling on top: draw last frame's visible sections, build a depth pyramid, test the
  rest against it in the same frame and draw the newly visible ones, so camera cuts never pop.
- Translucent draws are filtered by the graph on the CPU and by the occlusion result on the GPU.
- Index-buffered quads so each corner is shaded once instead of twice.
- Faces between two enclosed leaf blocks are dropped (More Culling-style), keeping the fancy surface look.
- `mcr --world` gains `--bench`, `--bench-view`, `--bench-modes`, `--no-occlusion` and `--no-graph` for
  reproducible fixed-view GPU timing and culling A/B images.

Out of scope: entity and block-entity culling, fog-distance culling, per-section translucent sorting, merging
water surface quads (fluid mesher), LOD, region-level (Nvidium-style) hierarchical culling.

## Capabilities

### New Capabilities
- `section-culling`: which terrain sections are drawn each frame beyond frustum culling (visibility graph,
  Hi-Z occlusion, translucent filtering) and the correctness guarantees around it.

### Modified Capabilities
- `chunk-meshing`: meshes carry face-to-face connectivity; enclosed leaf faces are culled.
- `world-viewer`: benchmark mode and culling switches on the command line.

## Impact

`mcr-render` (renderer frame structure, cull shaders, new graph and Hi-Z modules, indexed draws, public
`SectionVisibility`, `RendererConfig::{occlusion_culling, visibility_graph}`, `FrameStats::graph_ms`,
`GpuTimes::frame_ms`), `mcr-mesh` (connectivity flood fill, `MeshOptions::leaves_culling`), `mcr-app` (bench
mode, camera-in-block flag). About 30 MB more GPU memory for late-phase draw commands and the pyramid.
