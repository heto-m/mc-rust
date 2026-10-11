# world-viewer Specification

## Purpose
Defines how the client streams a saved world around the camera through meshing and rendering, and its command line, screenshot mode and stats.

## Requirements

### Requirement: Stream a world around the camera
The viewer SHALL load chunk columns of a world source on background threads, insert them into the chunk store on the main thread, and unload columns farther than the render distance plus two chunks. It SHALL load every column within the render distance of the camera, plus every column bordering one of them, so each column inside the distance has all eight neighbours loaded.

#### Scenario: Columns follow the camera
- **WHEN** the camera moves to another chunk
- **THEN** columns entering the render distance or its border ring are requested, and columns beyond distance + 2 are removed from the store and their meshes removed from the renderer

#### Scenario: Neighbours of in-range columns
- **WHEN** a column lies within the render distance
- **THEN** all eight columns around it are requested

#### Scenario: Missing chunks
- **WHEN** a requested chunk does not exist in the save
- **THEN** it is not requested again until the camera leaves and re-enters range

### Requirement: Mesh and upload pipeline
Inserting or removing a column SHALL mark dirty the sections of every loaded column in the 3x3 around it. Dirty sections SHALL be meshed on worker threads, nearest to the camera first, and uploaded within a per-frame time budget. A section SHALL be meshed and drawn only while its column and all eight neighbouring columns are loaded, checked only for columns dirtied that frame.

#### Scenario: Frame time stays bounded
- **WHEN** many meshes finish in one frame
- **THEN** only as many are uploaded as fit the budget and the rest wait for later frames

#### Scenario: Biome colours
- **WHEN** a world uses biomes not seen before
- **THEN** affected sections are remeshed with colours for those biomes

#### Scenario: Edge of the loaded region
- **WHEN** a column has an unloaded neighbour, including a diagonal one
- **THEN** it is not meshed, and no faces are emitted toward the missing column

#### Scenario: Neighbour arrives
- **WHEN** the last missing neighbour of a column is inserted
- **THEN** all of that column's sections are queued for meshing

#### Scenario: Neighbour unloads
- **WHEN** a neighbour of a meshed column is removed
- **THEN** the column's sections are removed from the renderer, and mesh results still in flight for it are discarded

### Requirement: Command line
`mcr --world <path>` SHALL render that save with `--render-distance`, `--no-vsync` and `--jar` options; `--synthetic` SHALL keep the generated terrain mode; `--connect` SHALL start the playable client.

#### Scenario: Screenshot
- **WHEN** `--screenshot out.png --camera x,y,z,yaw,pitch` is given
- **THEN** the viewer renders offscreen once every column in range is loaded and meshed, writes a PNG and exits

#### Scenario: Connect mode
- **WHEN** `--connect host:port` is given
- **THEN** the playable client starts instead of the viewer

### Requirement: Stats
The viewer SHALL print fps, frame time, meshed and uploaded section counts, quads, mesh queue length and GPU memory once per second.

#### Scenario: Periodic report
- **WHEN** a second elapses while running
- **THEN** one stats line is printed to stdout

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
