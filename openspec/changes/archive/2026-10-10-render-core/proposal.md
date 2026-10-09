# Proposal

## Why

The client needs a renderer foundation that can draw millions of voxel quads per frame far faster than
vanilla+Sodium. Every later feature (meshing, entities, assets) builds on its data formats and GPU
pipeline, so they must be fixed now.

## What Changes

- New crate `mcr-render` on wgpu: packed 8-byte cube quads and 32-byte model quads pulled from storage
  buffers, with a Rust encoder and WGSL decoder tested against each other.
- Arena sub-allocation per format, batched staging uploads, per-section GPU metadata.
- Compute pass doing frustum and per-face-direction culling that writes indirect draws and a count;
  `multi_draw_indirect_count` when available, `multi_draw_indirect` otherwise. Hi-Z seam designed only.
- Block texture 2D array with mips and a procedural placeholder.
- Reverse-Z infinite projection; f64 camera position with camera-relative section origins.
- Offscreen and surface targets; synthetic terrain generator and test mesher.
- `mcr-app` opens a winit window with a fly camera, vsync toggle and per-second frame stats.

Out of scope: real meshing, asset loading, Hi-Z implementation, translucency, entities, UI, lighting engine.

## Capabilities

### New Capabilities
- `voxel-rendering`: GPU-driven rendering of packed voxel geometry with culling, large-coordinate camera
  precision and offscreen readback.

### Modified Capabilities

## Impact

New crate `crates/mcr-render`; `crates/mcr-app` gains wgpu/winit usage. New dependencies: wgpu, winit,
glam, bytemuck, pollster, png (dev).
