## 1. Spec

- [x] 1.1 Proposal, design, spec delta, tasks; `openspec validate mod-dynamic-lights --strict`

## 2. ABI and Rust bridge

- [x] 2.1 ABI 20: `SHARED_LIGHTS`, `LIGHT_*` layout and flags in `abi.rs` and `BridgeAbi.java`, constants test
- [x] 2.2 `lights.rs`: shared block, sequence-locked read, bounds and clamps; unit tests with hostile blocks
- [x] 2.3 `FrameLink`: read the block when Java's frame completes; `dynamic_lights()` and `dynamic_light_provider()`

## 3. Java

- [x] 3.1 `McrDynamicLights`: find LambDynamicLights by reflection, collect entity, particle and behaviour sources,
      write the block in `frameDone`; disable on failure
- [x] 3.2 `cargo xtask java-build`, both loaders

## 4. Renderer

- [x] 4.1 `DynamicLight`, `Renderer::set_dynamic_lights`, frustum cull, nearest-64 selection and the light grid
      (unit tests)
- [x] 4.2 `dynlight.wgsl` variant entry points sharing the base decode; `RendererConfig::dynamic_lights`
- [x] 4.3 Variant pipelines and bind group bound only in frames with sources (the GPU test builds and validates them)
- [x] 4.4 Offscreen test: a synthetic light brightens terrain near it and nothing far away

## 5. Setting and wiring

- [x] 5.1 `mcrDynamicLights` option, Video Settings toggle while a provider is reported
- [x] 5.2 Shell passes the sources to the renderer in mod mode

## 6. Verification

- [x] 6.1 Fabric, live offscreen at night: held torch, dropped glowstone, burning zombie, beacon beam, mode off,
      mcr's switch off (screenshots in `run/shots/`)
- [x] 6.2 NeoForge: held torch, dropped glowstone, burning zombie, mode off, mcr's switch off
- [x] 6.3 Cost: `MCR_MOD_PROFILE=frame` and GPU timings with and without sources, and without mods

## 7. Wrap-up

- [x] 7.1 Policy: LambDynamicLights no longer partial; docs (`mod-bridge.md`, `mod-compat.md`)
- [x] 7.2 fmt, clippy, tests; archive
