# world-viewer Specification

## Purpose
Defines how the client streams a saved world around the camera through meshing and rendering, and its command line, screenshot mode and stats.

## Requirements

### Requirement: Stream a world around the camera
The viewer SHALL load chunk columns of a world source within the render distance of the camera on background threads, insert them into the chunk store on the main thread, and unload columns farther than the render distance plus two chunks.

#### Scenario: Columns follow the camera
- **WHEN** the camera moves to another chunk
- **THEN** columns entering the render distance are requested and columns beyond distance + 2 are removed from the store and their meshes removed from the renderer

#### Scenario: Missing chunks
- **WHEN** a requested chunk does not exist in the save
- **THEN** it is not requested again until the camera leaves and re-enters range

### Requirement: Mesh and upload pipeline
Inserting a column SHALL mark its sections and neighbouring sections dirty; dirty sections SHALL be meshed on worker threads, nearest to the camera first, and finished meshes SHALL be uploaded within a per-frame time budget.

#### Scenario: Frame time stays bounded
- **WHEN** many meshes finish in one frame
- **THEN** only as many are uploaded as fit the budget and the rest wait for later frames

#### Scenario: Biome colours
- **WHEN** a world uses biomes not seen before
- **THEN** affected sections are remeshed with colours for those biomes

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
