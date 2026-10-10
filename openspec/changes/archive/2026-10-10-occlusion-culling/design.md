# Design

## Context

See proposal.md. Before this change one compute dispatch did frustum and per-direction back-face culling
and wrote non-indexed indirect draws (6 vertices per quad) for every frustum-visible section; translucent
draws were built on the CPU after a frustum test. Measurements showed the horizon frame was about 80%
geometry-bound (4x fewer pixels only saved 15%), so the work had to cut submitted geometry.

## Goals / Non-Goals

**Goals:** cull hidden sections without visible popping or image changes, keep the CPU cost per frame
near zero, and measure every layer separately.

**Non-Goals:** per-quad or meshlet culling, LOD, entity culling, fog-distance culling.

## Decisions

### Visibility graph first (CPU)
- The mesher flood-fills the non-opaque (`!isSolidRender`) cells of each section and connects every pair of
  boundary faces a component touches; fewer than 256 opaque cells short-circuits to "all connected". The
  result is a 36-bit face-pair matrix (`SectionVisibility`) on `SectionMesh`. Empty meshes still carry it,
  so the renderer keeps graph nodes for sections without geometry; all-air sections are never meshed and
  count as open, as do unloaded ones.
- Search rule (vanilla/Sodium): leave a section only through faces connected to an entry face, and only in
  a direction that moves away from the camera section along that axis. The search ignores the frustum, so
  it is reused while the camera rotates and only reruns when the camera changes section or the graph
  changes; frustum, back-face and occlusion tests stay on the GPU.
- Because every step increases the Manhattan distance and only moves outward, a section can only be entered
  from its three camera-ward neighbours inside each octant. The search is therefore an outward sweep of the
  eight octants over a dense grid (no queue), and the dense grid is updated in place when nodes change.
  About 0.5 ms for 65x65x24 sections at render distance 32 (was 1.8 ms with a hash-map BFS).
- Result: a bitset over GPU slots uploaded to a storage buffer; the cull shaders skip unreached slots, and
  the CPU translucent list skips them too.
- Skipped while the camera is inside an opaque full block (spectator x-ray), like vanilla.
- Alternative: GPU wavefront BFS. Rejected: ~100 dependent dispatches per search for a 0.5 ms CPU job that
  runs only on section changes.

### Two-phase Hi-Z on top (GPU)
- Per-slot visibility word. Early phase: draw frustum- and graph-visible sections whose word is set. Then a
  compute pass builds the pyramid and the late phase tests every frustum- and graph-visible section, draws
  the visible ones not drawn early, and rewrites the word.
- Pyramid level 0 is half the depth size rounded up to powers of two (1024x1024 at 1080p, 11 levels), R32F,
  each texel the minimum (reverse-Z farthest) of its 2x2 footprint with clamped reads. Because every level
  halves exactly, a level-m texel covers exactly `2^(m+1)` screen pixels, so the test is exactly conservative.
- Test: project the 8 box corners; any corner behind the near plane means visible. Pick the finest level
  where the clamped pixel rectangle spans at most 2x2 texels, take the minimum of 4 loads, and call the box
  visible if its nearest depth is >= that minimum. Boxes are padded 1 block below and 2 above the 16^3 cell
  grid because model quads extend that far.
- Late draws go to a second region of the same command buffers with their own counters; draw data indices
  continue after the early count (early and late sets are disjoint, so the existing per-slot capacity holds).
- Translucent draws are sorted on the CPU; a small compute pass after the late cull zeroes the instance count
  of commands whose section failed the test, keeping the order.
- Alternative: reproject last frame's pyramid in a single phase. Rejected: misses disocclusions for a frame
  (popping on camera cuts).

### Indexed quads
A static u32 index buffer `(0,1,2),(2,1,3)` per quad (131072 quads, 3 MB); vertex `4q+k` is quad q corner k.
Faces split along the other diagonal swap corners with `k ^ 1`, which keeps the winding. Vertex reuse cuts
vertex shader invocations by a third: 1.36 -> 0.93 ms GPU on the forest horizon without culling. Model and
translucent sections beyond 131072 quads are truncated (hostile-input bound).

### Leaves
Faces between two leaf cells are dropped only when both cells are enclosed by leaves or opaque blocks. A
looser More Culling-style depth rule (drop when the neighbour leaf is backed by another leaf) saved ~10% in
forest views but let sky show through canopies from below, so it was rejected.

### Alternatives measured and not adopted
- Removing the alpha-test discard (an opaque-leaves fast path) saved ~3% without culling; not worth a second
  pipeline and quad split.
- Front-to-back sorting: slots are allocated in meshing-priority order (near first), so early draws are
  already roughly front to back; fragment work is ~20% of a horizon frame.
- Fog-distance culling: the world already streams a circle of columns, and culling fully fogged sections
  shows sky instead of fog colour where they rise above the horizon, so images would change.

## Measurements

RX 6800 XT, RADV, release, 1920x1080 offscreen, render distance 32 (3313 columns, 14.4M quads), test save
from `run/server-26.3`, `--bench 600 --bench-modes`, median GPU frame time (timestamps) and wall fps.

| View | Before (fps / GPU ms) | No culling* | Graph only | Hi-Z only | Graph + Hi-Z (fps / GPU ms) |
|---|---|---|---|---|---|
| 0 forest horizon, ground | 738 / 1.40 | 0.90 | 0.75 | 0.27 | 2563 / 0.25 |
| 1 horizon east | 718 / 1.48 | 0.96 | 0.65 | 0.36 | 2649 / 0.34 |
| 2 horizon over ocean | 923 / 1.21 | 0.73 | 0.49 | 0.45 | 2258 / 0.41 |
| 3 elevated horizon | 691 / 1.49 | 0.97 | 0.75 | 0.69 | 1460 / 0.65 |
| 4 looking down | 3044 / 0.39 | 0.23 | 0.12 | 0.16 | 3520 / 0.15 |
| 5 looking up | 4879 / 0.09 | 0.09 | 0.07 | 0.12 | 4250 / 0.12 |
| 6 large cave | 733 / 1.39 | 0.91 | 0.07 | 0.26 | 5098 / 0.10 |
| 7 large cave, down | 1199 / 1.07 | 0.63 | 0.08 | 0.19 | 4988 / 0.11 |
| 8 small cave | 678 / 1.55 | 1.04 | 0.07 | 0.16 | 4922 / 0.11 |
| 9 inside stone (x-ray) | 795 / 1.31 | 0.86 | 0.86 | 0.69 | 1377 / 0.69 |

*No culling = this change's indexed quads and leaf culling with both culling layers off.

Images: Hi-Z on vs off is pixel-identical in all views and in the first frame after each camera cut
(except one pixel where two coplanar faces tie in depth and draw order changed). The graph changes 16 and 2
pixels in the cave views: single-pixel T-junction cracks that now show different geometry behind them.
Water in the ocean view costs 0.14 ms of 0.65 ms because fluid surfaces are not greedy-merged.

## Risks / Trade-offs

- [Graph search is an approximation; a view that bends back toward the camera through a cave is cut] ->
  same behaviour as vanilla and Sodium; switchable at runtime.
- [Hi-Z adds ~0.03-0.05 ms fixed cost (pyramid, extra pass)] -> visible only when almost nothing is drawn
  (looking at the sky), where the frame is CPU-bound anyway.
- [Occluded translucent sections still issue empty indirect draws when only Hi-Z culls them] -> the graph
  removes most on the CPU; a GPU compaction pass can follow if needed.
- [Graph search 0.2-0.5 ms whenever the camera crosses a section boundary or meshes change] -> runs on the
  render thread; could move to a worker with one frame of latency if it shows up in frame pacing.
