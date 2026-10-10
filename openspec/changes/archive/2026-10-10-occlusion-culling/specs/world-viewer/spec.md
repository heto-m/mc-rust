## ADDED Requirements

### Requirement: Benchmark mode
After the world settles in screenshot mode, `--bench N` SHALL render each `--bench-view` pose (or the main
camera) for N frames, print fps, median GPU frame time, CPU render time, graph search time and draw counts,
and save each view's image, with animated textures held at a fixed tick.

#### Scenario: Fixed views
- **WHEN** `mcr --world <save> --screenshot out.png --bench 300 --bench-view 0,80,0,0,0` runs
- **THEN** it prints one timing line for the view and writes `out-view0.png`

### Requirement: Culling comparison
`--bench-modes` SHALL repeat the benchmark for no culling, graph only, occlusion only and both, also saving
the first frame after every camera change, and `--no-occlusion` and `--no-graph` SHALL disable either
culling layer in any mode of the viewer.

#### Scenario: Compare culling modes
- **WHEN** the benchmark runs with `--bench-modes`
- **THEN** it prints a line per view and mode and writes settled and first-frame images for each
