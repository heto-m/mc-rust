# mod-render-backend Specification

## Purpose
A renderpearl GPU backend (`com.mojang.renderpearl`) for the embedded client: Java records devices, resources,
copies and render passes as handles and word streams over FFM, and Rust validates them and executes them on wgpu
(SPIR-V translated through naga), so vanilla and mod rendering runs without OpenGL.

## Requirements

### Requirement: Renderpearl backend over wgpu
The bridge SHALL provide a `GpuDeviceBackend` implementation, wrapped by vanilla's `FrontendGpuDevice`, that creates every resource on the shared wgpu device and records all copies, clears, writes and render passes in the order vanilla issues them.

#### Scenario: Write after draw keeps order
- **WHEN** Java draws with a uniform buffer, then writes new data into it, then draws again in the same submit
- **THEN** the first draw sees the old data and the second sees the new data

### Requirement: SPIR-V shader translation
Pipelines SHALL be built from the SPIR-V that vanilla's frontend produces. Combined image samplers SHALL be split into separate texture and sampler bindings, and push constants SHALL be emulated with a uniform block, before naga translation. Pipelines using texel buffers SHALL fail compilation through vanilla's null-pipeline path without crashing.

#### Scenario: Vanilla pipelines compile
- **WHEN** the client loads its shaders through the bridge backend
- **THEN** every vanilla core and post pipeline without a texel buffer compiles, and the count of failures is zero

#### Scenario: Texel buffer pipeline
- **WHEN** the cloud pipeline (texel buffer) is compiled
- **THEN** compilation reports failure and the client keeps running

### Requirement: Handle safety
Java SHALL reference Rust resources only through generation-checked handles. Using a destroyed or stale handle SHALL produce a reported error and SHALL NOT access freed memory.

#### Scenario: Use after close
- **WHEN** Java binds a texture view whose texture was closed
- **THEN** Rust reports an invalid-handle error and skips the draw

### Requirement: Batched pass recording
Render pass commands SHALL be recorded into an off-heap command stream and replayed by Rust with one downcall per pass. Bind groups SHALL be cached and SHALL use dynamic offsets for uniform buffer slices.

#### Scenario: GUI frame call count
- **WHEN** the test screen renders one frame
- **THEN** the number of render-pass downcalls equals the number of render passes in that frame

### Requirement: Readback and fences
`copyTextureToBuffer` callbacks SHALL run on the Java render thread after the GPU copy completes. A fence wait SHALL block only while a readback that was recorded at or before the fence's submission is still incomplete. Otherwise a fence wait SHALL return at once, since host writes reach the GPU as copies in queue order. A zero-timeout wait on the open recording's fence SHALL report not done without submitting it. Polling for readbacks SHALL NOT poll the device while no readback is pending.

#### Scenario: Screenshot
- **WHEN** Java takes a screenshot through vanilla's screenshot path
- **THEN** the callback runs within two frames and the saved image matches the offscreen image

#### Scenario: Ring buffer reuse
- **WHEN** a mapped ring buffer waits on the fence of the slot it reuses, and no readback is pending
- **THEN** the wait returns without polling the device, and the earlier draws still see the earlier data

### Requirement: Visual parity
The bridge backend's output for the test screen SHALL match vanilla's OpenGL output, captured locally and never committed, at SSIM ≥ 0.98.

#### Scenario: Golden comparison
- **WHEN** the test screen is rendered offscreen at 1280×720 through the bridge
- **THEN** its SSIM against the local OpenGL golden image is at least 0.98
