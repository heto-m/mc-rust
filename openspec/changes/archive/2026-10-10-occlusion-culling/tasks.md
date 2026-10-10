# Tasks

## 1. Benchmark

- [x] 1.1 `--bench`, `--bench-view` with wall fps, GPU frame timestamps and CPU time; fixed animation tick
- [x] 1.2 Baseline on the RD32 save: horizon, forest, ocean, elevated, up, down, caves, inside stone
- [x] 1.3 `--bench-modes`, `--no-occlusion`, `--no-graph` with per-mode and first-frame-after-cut images

## 2. Hi-Z occlusion

- [x] 2.1 Depth pyramid build (exact power-of-two levels, reverse-Z minimum) with level-size unit test
- [x] 2.2 Early/late cull entry points, per-slot visibility, late command region and counters
- [x] 2.3 Split world pass into early and late render passes; timestamps for all four passes
- [x] 2.4 GPU tests: identical frames on a camera path, fast turns and teleports, wall hides sections
- [x] 2.5 Translucent draws zeroed when their section fails the test

## 3. Indexed quads

- [x] 3.1 Shared quad index buffer; cube, model and translucent draws indexed; per-draw quad bounds
- [x] 3.2 Verify identical images and measure

## 4. Visibility graph

- [x] 4.1 Face connectivity flood fill in the mesher with unit tests
- [x] 4.2 Graph nodes for sections with and without geometry; octant sweep search with unit tests
- [x] 4.3 Reachability bitset in the cull shaders and the CPU translucent list
- [x] 4.4 Skip the search while the camera is inside an opaque block (app sets the flag)
- [x] 4.5 GPU test: sealed wall, reopening, camera inside a block
- [x] 4.6 Benchmark graph only, Hi-Z only and both; keep both

## 5. Leaves and overdraw

- [x] 5.1 Enclosed-leaf face culling with a unit test; compare against the looser depth rule
- [x] 5.2 Measure discard-free opaque path, translucent cost and fragment share; record in design.md

## 6. Wrap-up

- [x] 6.1 fmt, clippy, workspace tests, `openspec validate --all --strict`
