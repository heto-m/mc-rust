# Spec Delta

## Purpose

Defines how voxel geometry is encoded, stored, culled and drawn on the GPU so terrain rendering stays
fast and precise at any world coordinate.

## ADDED Requirements

### Requirement: Packed quad encoding
The renderer SHALL encode cube-face quads in 8 bytes and non-cube model quads in 32 bytes, and the shader
decoding SHALL yield exactly the fields the Rust encoder was given.

#### Scenario: Encoder and shader agree
- **WHEN** quads with boundary field values are encoded and decoded on the GPU
- **THEN** every decoded field equals the encoded value

#### Scenario: Out-of-range field rejected
- **WHEN** a field exceeds its bit width
- **THEN** encoding fails instead of corrupting neighbouring fields

### Requirement: Shared geometry arenas
Section geometry SHALL be sub-allocated from one GPU buffer per quad format, and freed ranges SHALL be
reusable without overlap.

#### Scenario: Replace section
- **WHEN** a section mesh is replaced or removed
- **THEN** its range is returned to the arena and later allocations never overlap live ranges

#### Scenario: Arena exhausted
- **WHEN** a section does not fit
- **THEN** the upload reports an error and existing sections stay intact

### Requirement: GPU culling
The renderer SHALL cull sections against the view frustum and skip face directions facing away from the
camera in a compute pass that emits indirect draw arguments and a draw count.

#### Scenario: Section behind camera
- **WHEN** a section lies entirely outside the frustum
- **THEN** it produces no draws

#### Scenario: Back faces skipped
- **WHEN** the camera is on the negative X side of a section
- **THEN** that section's +X faces are not drawn

### Requirement: Large-coordinate precision
Camera position SHALL be stored in f64 and sections positioned relative to the camera so rendering shows no
jitter at coordinates up to 30 million blocks.

#### Scenario: Far from origin
- **WHEN** the same scene is rendered at the origin and translated by 29,999,984 blocks
- **THEN** the images are identical

### Requirement: Offscreen rendering
The renderer SHALL render to an offscreen target and return RGBA pixels, and creation SHALL fail cleanly
when no GPU adapter exists.

#### Scenario: Synthetic terrain
- **WHEN** a synthetic terrain scene is rendered offscreen
- **THEN** the returned image contains non-uniform sky and terrain pixels

### Requirement: Interactive window
The application SHALL show the synthetic terrain in a window with a fly camera, optional vsync off, and
frame-time statistics printed each second.

#### Scenario: Run app
- **WHEN** the app runs
- **THEN** it opens a window, responds to WASD and mouse, and prints stats every second
