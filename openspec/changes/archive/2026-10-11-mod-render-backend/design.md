## Context

The renderpearl seam in 26.3 (`com.mojang.renderpearl`):

- `api.device.GpuBackend`: `getName`, `loadLibrary`, `unloadLibrary`, `createWindow(String,int,int,long)`,
  `createDevice(GpuDebugOptions)`. Both vanilla backends return `new FrontendGpuDevice(<GpuDeviceBackend>)`. The
  frontend (`FrontendGpuDevice`, `FrontendCommandEncoder`, `FrontendRenderPass`, `PipelineBuilder`) validates
  everything and maps uniform names to indices, so a backend sees pre-validated, index-based calls.
- `backend.api.GpuDeviceBackend`:
  - `createSurface(long, BooleanSupplier)`, `createCommandEncoder()`
  - `createSampler(AddressMode×2, FilterMode×2, int maxAnisotropy, OptionalDouble maxLod)`
  - `createTexture(label, usage, GpuFormat, w, h, depthOrLayers, mips)`, `createTextureView(texture, baseMip, mips)`
  - `createBuffer(label, usage, size | ByteBuffer)`
  - `compilePipeline(BackendRenderPipeline.CreateInfo) → Pending`
  - `createTimestampQueryPool`, `getTimestampCalibrationOffset`, `getDeviceInfo`, `close`
  - The frontend supports only 2D textures and cubemaps (3D and arrays throw).
- `backend.api.CommandEncoderBackend`:
  - `submit()`, `transientMemory()`, `createRenderPass(RenderPassDescriptor)`, `submitRenderPass()`
  - clears: color, color+depth (optionally a region and mip), depth
  - `writeToBuffer(slice, ByteBuffer)`, `copyToBuffer`, `writeToTexture(tex, ByteBuffer, mip, layer, x, y, w, h)`,
    `copyBufferToTexture`, `copyTextureToBuffer(…, Runnable callback, …)`, `copyTextureToTexture`
  - `createFence()`, `writeTimestamp`
  - The frontend allows a single open pass, and copies/clears only outside passes.
- `backend.api.RenderPassBackend`:
  - debug groups, `setPipeline`, `setUniform(int index, Object)` (a `GpuBufferSlice` for a UBO, a
    `TextureViewAndSampler` for a combined sampler), `pushConstants(ByteBuffer)`, scissor
  - `setVertexBuffer(slot, slice)`, `setIndexBuffer(buffer, IndexType SHORT|INT)`
  - `draw`, `drawIndexed`, two `multiDraw`/`multiDrawIndexed` forms, `drawIndirect`, `drawIndexedIndirect`,
    `writeTimestamp`
- `backend.api.GpuSurfaceBackend`: `configure(Configuration(w, h, PresentMode))`, `isSuboptimal`,
  `acquireNextTexture`, `blitFromTexture(encoder, view)`, `present`, `supportedPresentModes`.
- `BackendRenderPipeline.CreateInfo(name, shaders[(name, "main", SpvModule)], vertexBuffers(slot, stride,
  stepRate), attribBindings(slot, location, offset, GpuFormat), uniforms[UniformDescription(name, type, format)],
  pushConstantsSize, DepthStencilState?, PolygonMode, cull, colorTargetStates[], PrimitiveTopology)`.

Shader facts:
- Shaders are GLSL `#version 330` text (`assets/minecraft/shaders/{core,include,post}`, `.vsh`/`.fsh`).
- `frontend.shaders.GlslCompiler.compileToSpv` compiles them with LWJGL shaderc for Vulkan 1.0, with
  `auto_bind_uniforms`, defines `RENDERPEARL_DEPTH_IS_ZERO_TO_ONE` (when `DeviceInfo.isZZeroToOne`) and friends,
  and entry `main`.
- `PipelineBuilder` reflects with SPIRV-Cross and rewrites every descriptor to set 0 with binding = flattened
  uniform index.
- The GL backend decompiles the SPIR-V back to GLSL 330 (`GlPipelineRecompiler`). Vulkan consumes it directly.
- Uniform kinds are `UNIFORM_BUFFER`, `COMBINED_IMAGE_SAMPLER` and `TEXEL_BUFFER`. Layouts are declared in
  `net.minecraft.client.renderer.BindGroupLayouts`.
- Depth is reverse-Z (`DepthStencilState.DEFAULT` is `GREATER_THAN_OR_EQUAL`, cleared to 0.0) and `MainTarget` is
  `RGBA8_UNORM` + `D32_FLOAT`.

## Decisions

**Object model.** Each Java resource object extends the vanilla base class where one exists (`BaseGpuBuffer`,
`BaseGpuTexture`, `BaseGpuTextureView`) and stores a `long handle`. A handle is `index | generation << 32` into a
Rust slab of `wgpu::Buffer`/`Texture`/`TextureView`/`Sampler`/`RenderPipeline`. A stale handle is a reported
error, not UB. `close()` downcalls `destroy(handle)`, and Rust drops the object once the in-flight submissions using
it complete (wgpu keeps resources alive internally). Downcalls may come from any Java thread: pipeline compiles
run on vanilla's executor. Slabs are sharded behind `parking_lot` mutexes, and the record path touches only the
encoder's own state.

**Device sharing.** The backend uses `mcr-render`'s `wgpu::Device` and `Queue`, so textures can be shared with the
Rust renderer later. `DeviceInfo`:
- `isZZeroToOne = true`, `backendName = "mcr-wgpu"`, limits from `wgpu::Limits`
  (`minUniformOffsetAlignment = min_uniform_buffer_offset_alignment`, `maxColorAttachments`, `maxTextureSize`).
- Features: `multiDrawDirectInterleaved/Separate = true` (looped in Rust), `drawIndirect = true`,
  `multiDrawIndirect` from `wgpu::Features::MULTI_DRAW_INDIRECT_COUNT`/native support, `nonZeroFirstInstance = true`,
  `persistentMapping = false`, `shaderDrawParameters = false`, `wireframeFillMode` from `POLYGON_MODE_LINE`.

**Command recording.** A Java `McrCommandEncoder` owns a Rust encoder state: a lazily created
`wgpu::CommandEncoder` plus a staging belt.
- Copies and clears are direct downcalls recorded in order. Clears are empty render passes with load ops.
- Writes copy the caller's bytes into staging memory and record `copy_buffer_to_buffer` or
  `copy_buffer_to_texture` (rows repacked to 256-byte alignment) at that point in the stream, so ordering
  relative to passes is preserved. `queue.write_*` would hoist writes before the whole submit, so it is never used.
- `createRenderPass` returns `McrRenderPass`, which appends ops to a reusable off-heap stream:
  `[u8 op][payload]`, with ops `PIPELINE`, `UNIFORM_BUF`, `UNIFORM_TEX`, `PUSH`, `SCISSOR`, `NO_SCISSOR`, `VB`, `IB`,
  `DRAW`, `DRAW_IDX`, `MULTI*` (arrays inline), `INDIRECT*`, `DBG_PUSH`, `DBG_POP`.
- `submitRenderPass()` makes one downcall `encode_pass(encoder, descriptor, stream, len)`. On the calling Java
  thread, Rust opens the `wgpu::RenderPass` and replays the stream:
  - Bind groups are resolved lazily before each draw, from a cache keyed by (pipeline layout, resource handles,
    UBO sizes), with UBO offsets as dynamic offsets.
  - Draws are issued directly.
  - The cost per op is one enum match.
- `submit()` finishes the encoder. In this change it calls `queue.submit` directly. `mod-frame-compose` routes the
  buffer to a frame slot instead.

**Bindings.** Pipeline layout: one bind group (set 0). For flattened uniform index `i`:
- `UNIFORM_BUFFER` → binding `2i`, uniform buffer with dynamic offset.
- `COMBINED_IMAGE_SAMPLER` → texture at binding `2i` and sampler at `2i+1`.
- Push constants → an extra uniform block at binding `2n`, filled per call from transient memory, exactly as
  `GlRenderPass.pushConstants` does through `uploadGpu`.
- `TEXEL_BUFFER` → compile fails (`Pending.NULL`, the path vanilla already handles when GL decompilation fails).

**SPIR-V rewrite (pass `spv_fixup`).** naga's SPIR-V frontend maps `OpTypeSampledImage` to the bare image type and
has no sampler to bind for a combined-sampler global. A small word-level pass fixes that:
1. Combined sampler globals (`UniformConstant` of `OpTypeSampledImage`) become an image global (binding `2i`) plus
   a new `OpTypeSampler` global (binding `2i+1`).
2. Every `OpLoad` of the old global becomes two loads plus `OpSampledImage`.
3. Each UBO's binding becomes `2i`.
4. `PushConstant` blocks become `Uniform` blocks at binding `2n`.

The pass is checked by running naga validation on all vanilla shaders in a unit test that reads them from the
local `client.jar`. Then naga `spv-in` (`adjust_coordinate_space` chosen to match vanilla's GL-style Y-up clip
space, verified by the golden test) → `wgpu::ShaderSource::Naga`.

**State mapping.**
- `GpuFormat` → `wgpu::TextureFormat`/`VertexFormat`:
  - 3-component 8/16-bit vertex formats map to their 4-component form, which is safe because vanilla vertex
    formats pad to 4 bytes. Checked per attribute against the stride.
  - `D24_UNORM_S8_UINT` → `Depth24PlusStencil8`.
  - RGB texture formats are rejected.
- `CompareOp`, `BlendFactor`/`BlendOp`, write masks and `DepthStencilState` depth bias (constant, slope) map
  directly. Cull is back faces with CCW front.
- `PolygonMode.LINE` maps to line mode when the feature is present, else fill with a warning.
- Topology follows vanilla's Vulkan mapping:
  - `LINES`/`QUADS`/`TRIANGLES` → `TriangleList` (vanilla expands lines and quads with index buffers).
  - `DEBUG_LINES` → `LineList`, `DEBUG_LINE_STRIP` → `LineStrip`, `POINTS` → `PointList`,
    `TRIANGLE_STRIP` → `TriangleStrip`.
  - `TRIANGLE_FAN` → `TriangleList`, with non-indexed fan draws converted through a generated index buffer.
- Vertex step rate `0/1` → `VertexStepMode::Vertex/Instance`. Scissor and viewport origin follow `VulkanRenderPass`.

**Transient memory.** `McrTransientMemory` implements every `TransientMemory` method over per-encoder ring arenas
that Rust owns and reclaims after the submit that used them completes.
- `allocateCpu` → a direct `ByteBuffer` in a CPU arena.
- `allocateStaging`/`allocateGpuMapped` → a `MappedView` over staging memory. Its `onClose` records the copy into
  a transient GPU buffer slice.
- `uploadGpu`/`uploadStaging`/`multiUpload*` → the same, with one copy per list element.

**Mapping and readback.**
- `GpuBuffer.map(offset, len, read, write)`:
  - Write maps return a view of a Rust shadow allocation; close records a staged copy of that range.
  - Read maps are allowed only on `USAGE_MAP_READ` buffers after their copy completed. They return the Rust
    readback copy.
- `copyTextureToBuffer(…, Runnable callback, …)`: the encoder records the copy, and `submit()` registers
  `map_async` on the readback.
- Rust polls the device in `awaitFrame` and in fence waits, and posts completion ids to the Java control ring.
  Java runs each callback on its thread.
- `createFence()` returns a fence holding the submission index. `awaitCompletion` is a blocking downcall
  `device.poll(Wait(index))`.
- `GpuSurfaceBackend` (offscreen): `configure` sizes an `Rgba8Unorm` image, `blitFromTexture` records a copy, and
  `present` stores the image for readback by tests.

**Per-frame data.** A frame crosses as:
- one command stream per pass (16–40 bytes per op);
- staged writes: dynamic uniforms (`DynamicGpuData`, about 160 bytes per draw), `StagedVertexBuffer` vertex data,
  sprite animation uploads;
- one downcall per pass, copy and resource lifecycle event.

The expected GUI frame is a few hundred ops and under 1 MiB of uploads. Task 4.2 measures it.

## Measured

- **Spike 0.1 (shaders).** Instead of a separate shaderc tool, the bridge dumps every `CreateInfo` the real frontend
  produces (`-Dmcr.gpu.dump`, `-Dmcr.gpu.device=null`) into `.cache/spv/26.3`. Of 216 pipelines, 208 pass
  `spv_fixup` + naga validation and 8 use texel buffers (all clouds); 0 fail. Besides the planned rewrite, two
  more fixups were needed: output arrays (OIT's `out vec4 coeff[N]`) become N consecutive outputs, and debug
  instructions are dropped (naga rejects `OpLine` between types).
- **Live compile (`gpu_compile::vanilla_pipelines_compile_on_wgpu`).** Through the backend on wgpu: core 184/184,
  post 24/24, texel 8 (fail cleanly), 0 failures. This needed `FLOAT32_BLENDABLE` (OIT depth-bounds pipelines blend
  into `RGBA32_FLOAT`) and binding uniform blocks at their std140 size: vanilla sizes buffers without tail padding
  (140 vs 144), so uniform-capable buffers are padded to 16 bytes and bindings round up to the block size.
- **Coordinates.** naga's `adjust_coordinate_space` flips clip-space Y, which reproduces vanilla's Vulkan memory
  layout (row 0 = bottom, as in GL). Front faces are therefore CW in wgpu terms, scissor rectangles pass through
  unchanged, and the offscreen surface flips rows on readback (vanilla's Vulkan blit flips likewise).
- **Handles** carry their kind (`generation << 32 | kind << 28 | index`), so a handle of the wrong type is as
  invalid as a stale one. Java defers destroys to the next submit, matching Vulkan's destroy queue.
- **Transient memory** uses vanilla's `TransientBlockAllocator` over Rust blocks with a host copy. Allocations mark
  their range dirty; Rust uploads dirty ranges with `queue.write_buffer` at submit, which wgpu orders after earlier
  submits, so blocks recycle immediately. Persistent-buffer writes and texture writes go through an upload ring
  and `copy_*` commands at their place in the stream.
- **Golden (`cargo xtask golden-gui`).** The stock Fabric client renders the test mod's parity screen (gradient, bars
  behind the menu blur, buttons, shadowed text, item icons with counts, a scissored list) with vanilla's OpenGL
  backend under Xvfb (Mesa 26.2 llvmpipe) into `.cache/golden/26.3/gui.png`. Through the bridge on wgpu
  (`gpu_golden::gui_matches_the_opengl_golden`): SSIM 0.9996 against the golden; the Java screenshot (a readback
  through `copyTextureToBuffer`) matches the presented frame at SSIM 1.0000; 0 validation errors.
- **Bench (`gpu_bench::bench_gui_frames`, 1280×720, 120 frames after a 30-frame warm-up, per frame).** `record` is
  wall time from pass creation to its end on the render thread (vanilla's own GUI work included), `native` the FFM
  call, `encode` Rust's share of it.

  | Screen | passes | ops | draws | record | native | encode | bind hits | upload |
  |---|---|---|---|---|---|---|---|---|
  | Title (panorama) | 4.2 | 558 | 78 | 55 µs | 60 µs | 56 µs | 99.8% | 26 KiB |
  | Test screen (blur) | 10.2 | 598 | 85 | 81 µs | 78 µs | 70 µs | 100.0% | 30 KiB |

  Both screens are capped at 60 fps by vanilla's menu limit; recording plus encoding stays well under 0.2 ms of the
  16.7 ms frame, so bind group churn is not a concern for GUI frames.

## Risks / spikes

- **Combined samplers through naga** (spike 0.1): if the rewrite cannot make all vanilla shaders validate, the
  fallback is per-pipeline WGSL overrides for the failing ones, or SPIR-V passthrough on the Vulkan backend only.
- Coordinate conventions (clip-space Y, render-target sampling orientation, scissor origin). The golden test
  catches mismatches, and fixes stay in one place (the naga option or a viewport flip).
- Pipeline color formats: Java declares `RGBA8_UNORM` targets. If they later alias Rust targets of other formats,
  `mod-frame-compose` compiles per-format variants lazily.
- Bind group churn in GUI-heavy frames: mitigated by the cache (99.8–100% hits in task 4.2).
