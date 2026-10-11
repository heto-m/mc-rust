# Proposal

## Why

Java must draw GUI, HUD, screens, entities and mod overlays without OpenGL. Vanilla 26.3 already abstracts the
GPU behind `com.mojang.renderpearl` with OpenGL and Vulkan backends. Backends receive SPIR-V (shaderc on the Java
side) plus explicit pipeline state, so a third backend that records wgpu work on our device is the natural seam.
This change builds that backend and proves it offscreen. Frame integration comes in `mod-frame-compose`.

## What Changes

- Java side: `McrDeviceBackend` implementing `GpuDeviceBackend`, plus `CommandEncoderBackend`, `RenderPassBackend`,
  `GpuSurfaceBackend`, `BackendRenderPipeline`, `TransientMemory`, buffers, textures, views and samplers.
  - Objects are thin Java wrappers holding generation-checked Rust handles.
  - Render pass commands are encoded into an off-heap command stream, with one downcall per pass.
  - `McrGpuBackend` switches from the null device (`mod-fabric-boot`) to this one.
- Rust side, `mcr-modbridge::gpu`:
  - Handle slabs over the shared `wgpu::Device`/`Queue`, and in-order recording of copies, clears and passes onto
    a `wgpu::CommandEncoder`.
  - A staging belt for writes, readbacks with completion callbacks, and fences.
  - Bind group caching with dynamic offsets.
- Shader path: SPIR-V from `BackendRenderPipeline.CreateInfo` → a Rust pass that splits combined image samplers
  and turns push constants into a uniform block (as the GL backend does) → naga `spv-in` → wgpu.
  - Pipeline state maps 1:1 (blend, depth/bias, cull, write masks, vertex layouts, topology).
- Offscreen surface: `blitFromTexture` and `present` produce a readable image, for tests.

Out of scope:
- Composition into the mcr window, frame lockstep and virtual targets (`mod-frame-compose`).
- Texel buffers (`UNIFORM_TEXEL_BUFFER`, used only by vanilla clouds, which Rust draws natively): pipeline compile
  fails cleanly.
- Timestamp queries.
- Persistent mapping.
- Mods calling OpenGL directly.

Acceptance test:
- Every vanilla core and post pipeline that does not use texel buffers compiles through the backend, with a
  printed count and zero failures.
- A test screen from `java/mcr-testmod` (text, buttons, item icons, a scissored scroll list, blur) renders
  offscreen at 1280×720.
- That image matches a golden image captured locally from vanilla's OpenGL backend (gitignored under `.cache`,
  never committed) at SSIM ≥ 0.98.

## Capabilities

### New Capabilities
- `mod-render-backend`: renderpearl backend over wgpu, shader translation, resource ownership, command
  recording and offscreen presentation.

### Modified Capabilities

## Impact

- `mcr-modbridge` gains `gpu/` (deps: `wgpu` (workspace version), `naga` with `spv-in`, `rspirv` or a small hand
  parser for the SPIR-V rewrite).
- `mcr-render` shares its `Device`/`Queue` and requests the extra features only in mod mode.
