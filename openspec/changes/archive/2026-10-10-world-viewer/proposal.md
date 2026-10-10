## Why

The renderer, mesher, world store and Anvil reader exist but have never run together. Rendering a real vanilla
world from disk is the first end-to-end proof of the pipeline and the engine loop the network client will reuse.

## What Changes

- Restructure `mcr-app` into `Game` (chunk store, camera) and `RenderSystem` (renderer, mesh pool, uploads), driven by a shared engine loop.
- Add a `WorldSource` trait with an `AnvilSource` that streams chunk columns around the camera on background threads.
- Wire chunk insertion to dirty sections, meshing jobs, time-budgeted uploads and unloading.
- CLI: `mcr --world <save> [--render-distance N] [--no-vsync] [--jar <jar>] [--screenshot out.png --camera x,y,z,yaw,pitch] [--synthetic]`.
- Stdout stats once per second.

## Capabilities

### New Capabilities
- `world-viewer`: streaming a saved world through the meshing and rendering pipeline, with offscreen screenshots.

### Modified Capabilities

## Impact

`crates/mcr-app` only; depends on `mcr-anvil`, `mcr-world`, `mcr-mesh`, `mcr-assets`, `mcr-render`.
