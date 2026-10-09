# Design

## Context

See proposal.md. Target: wgpu 30, winit 0.30 (latest stable), Vulkan first (RADV dev machine).

## Goals / Non-Goals

**Goals:** minimal CPU work per frame (one compute dispatch, two indirect multi-draws), no per-section
buffers, exact positioning at large coordinates.
**Non-Goals:** Hi-Z implementation, translucency, real textures, meshing.

## Decisions

### Cube quad (8 bytes, two u32)
- word0: x[0:5) y[5:10) z[10:15) block coords in the section, face[15:18) (0 +X,1 -X,2 +Y,3 -Y,4 +Z,5 -Z),
  w-1[18:22), h-1[22:26), bits 26..32 reserved.
- word1: texture layer[0:16), AO 4x2 bits[16:24), light[24:32) (sky<<4 | block).
- Quad extent: u/v tangent axes are (Z,Y) for X faces, (X,Z) for Y faces, (X,Y) for Z faces; w spans u,
  h spans v. Corners 0..3 = (0,0),(1,0),(0,1),(1,1) in (u,v). The AO diagonal is flipped in the shader.
- Alternatives: 16-bit fields at 1/16 precision need 16 bytes; rejected.

### Model quad (32 bytes, eight u32)
words 0..3 corner positions, 10 bits per axis at 1/32 block with a -1 block bias (range -1..31);
words 4..5 corner UVs (u8,v8 each, 1/16 tile units, up to 16 tiles); word6 layer[0:16) light[16:24)
shade normal[24:27) flags[27:32); word7 AO 4x2 bits, rest reserved. Drawn as 6 vertices per quad.

### Vertex pulling with a draw table
Draws are `vertex_count = 6*quads`, `first_instance = draw slot`. The cull pass writes, per visible draw, an
indirect command and a `DrawData {origin_rel, first_quad}` at the same slot; the vertex shader reads
`draws[instance_index]` and `quads[first_quad + vertex_index/6]`. This needs `INDIRECT_FIRST_INSTANCE`,
which every desktop GPU has, and avoids `draw_index`, which wgpu does not expose.

### Arena
Offset allocator in units of quads: BTreeMap free ranges with coalescing plus a (size, offset) set for
best-fit. Capacity is fixed per format and clamped to device `max_storage_buffer_binding_size`
(requested at adapter maximum).

### Uploads
A CPU staging ring (one reused byte vector per frame) records writes; at flush, adjacent writes to the same
buffer are coalesced and issued as `queue.write_buffer`.

### Section metadata and culling
64-byte `SectionMeta` per slot: integer block origin, flags, cube base, model base/count, six per-direction
counts (a section's cube quads are grouped by direction). One compute thread per slot tests the frustum
(5 planes in camera-relative space, infinite far) and per-direction visibility: +X faces can be seen only if
camera x > min_x+1, -X only if camera x < max_x-1, likewise Y/Z. Visible directions atomically append a
command. With `MULTI_DRAW_INDIRECT_COUNT` the counter buffer is the count; otherwise the command buffer is
cleared first and `multi_draw_indirect` runs over the live upper bound (empty commands are no-ops).

### Hi-Z seam
The cull shader has a `hiz_visible(min, max)` hook (currently returns true) and bind group 2 is reserved
for the previous-frame depth pyramid. Later: build pyramid after the main pass, test against reprojected
AABBs in the same dispatch, then re-test culled sections (two-phase).

### Precision
Camera is f64 plus orientation. Uniforms carry the integer part of the camera position (i32) and the
fractional part (f32); the GPU computes `f32(section_origin - cam_int) - cam_frac`, exact for any world
coordinate. View matrix is rotation only. Projection is reverse-Z infinite (depth cleared to 0, compare
Greater, Depth32Float).

### Textures
2D array with full mip chain; placeholder generated on the CPU with box-filtered mips. Nearest magnify,
linear mip, repeat addressing so merged quads tile.

### Timing
Timestamp queries around the compute and render passes when `TIMESTAMP_QUERY` exists, read back with a
frame of latency. GPU memory is reported from our own allocation accounting.

## Risks / Trade-offs

- Fixed arena capacity -> exhaustion reports an error; growth/paging later.
- No translucency or entity pass yet -> separate pipelines later reuse the same culling output pattern.
- Software Vulkan may lack MDI count -> fallback path covers it.

## Measurements

RX 6800 XT, RADV, release build, `cargo run --release -p mcr-render --example bench` (1920x1080, offscreen,
synthetic heightmap terrain, render distance 48 = 97x97 columns, 16016 sections, 4.44M cube quads and 0.2M
model quads, camera rotating 360 degrees, draw-count and timestamps both available):

- Frame time 0.46 ms (about 2200 fps, CPU-submission bound); CPU `render()` 0.43 ms; GPU cull 0.003 ms, draw 0.43 ms.
- About 12k cube draws and 3.5k model draws survive culling in the last frame.
- GPU memory: 192 MB cube arena + 16 MB model arena reserved (40 MB used), 36 MB culling buffers, 16 MB targets.
- Meshing the world with the test mesher takes 0.14 s on 16 threads; uploading takes 145 ms in one frame.
