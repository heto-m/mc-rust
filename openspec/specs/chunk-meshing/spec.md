# chunk-meshing Specification

## Purpose
Defines how chunk sections become packed render geometry.

## Requirements

### Requirement: Model cache
The mesher SHALL bake every block state once into a compact table giving its render layer, tint source,
occlusion and either a per-face sprite layer (full opaque cubes) or generic model quads, using a magenta
and black missing sprite for absent textures instead of failing.

#### Scenario: Known block classification
- **WHEN** the cache is built from the vanilla jar
- **THEN** stone is an opaque cube, glass and leaves are cutout or translucent as in vanilla, and water is translucent

#### Scenario: Missing texture
- **WHEN** a model references a texture that does not exist
- **THEN** the state still bakes and uses the missing sprite

### Requirement: Face culling and greedy merging
Cube faces SHALL be emitted only where the neighbour does not occlude them, and adjacent faces with the
same layer, light, tint and ambient occlusion SHALL merge into one quad covering the same area.

#### Scenario: Hidden face
- **WHEN** two opaque cubes touch
- **THEN** neither emits the shared face

#### Scenario: Merge preserves area
- **WHEN** a section is meshed with and without merging
- **THEN** the total covered face area per direction is equal

### Requirement: Ambient occlusion
Each cube face corner SHALL get an occlusion level from the two side blocks and the corner block using
vanilla's smooth-lighting rule (average of four shade terms, 0.2 for full-collision blocks and 1.0 otherwise),
and quads whose corner values are not constant along a merge direction SHALL NOT merge along it.

#### Scenario: Flat floor
- **WHEN** a face has no occluding neighbours
- **THEN** all four corners have the brightest level

#### Scenario: Concave corner
- **WHEN** both side neighbours of a corner occlude
- **THEN** that corner has the darkest level

### Requirement: Fluids
Water and lava SHALL be meshed like vanilla: corner heights from fluid levels and neighbours, flow-direction
texture rotation using the flowing sprite, the overlay sprite beside glass-like blocks, side and bottom faces
culled by occlusion, and waterlogged blocks rendering their water. Water SHALL go to the translucent layer.

#### Scenario: Source under air
- **WHEN** a water source has air above
- **THEN** its top face is lower than one block

#### Scenario: Flowing top face
- **WHEN** a fluid cell has a horizontal flow vector
- **THEN** its top face uses the flowing sprite rotated to the flow direction

### Requirement: Biome tints
Grass, foliage, dry foliage and water tints SHALL be averaged over a configurable biome blend radius
(default 2), sampled per block column with vanilla's seeded biome zoom, and selected per quad by tint kind.

#### Scenario: Blend radius zero
- **WHEN** the radius is 0
- **THEN** each column uses its own biome colour

#### Scenario: Zoom determinism
- **WHEN** the same quart biomes and seed are meshed twice
- **THEN** the biome chosen for every block column is identical

### Requirement: Background meshing
Dirty sections SHALL be snapshotted and meshed on worker threads, with results delivered without
blocking the caller.

#### Scenario: Non-blocking poll
- **WHEN** results are polled while workers are busy
- **THEN** the poll returns immediately with whatever is complete

### Requirement: Total coverage of block states
Meshing a section containing any block state SHALL NOT panic.

#### Scenario: Every state
- **WHEN** sections containing all block states are meshed
- **THEN** meshing completes

### Requirement: Per-vertex smooth lighting
Every cube and model quad SHALL carry four corner sky/block light pairs computed with vanilla's smooth
blend, so lighting can be recomputed per frame without remeshing, and greedy merging SHALL require equal
corner values.

#### Scenario: Corner light blend
- **WHEN** a floor cell has a dark neighbour on one side
- **THEN** the corners touching that neighbour have lower light than the opposite corners

#### Scenario: Merge needs equal light
- **WHEN** two adjacent faces differ in any corner light
- **THEN** they are not merged

### Requirement: Random variants and offsets
Weighted block-state variants SHALL be selected from the block position exactly as vanilla does
(`Mth.getSeed` into a Java-compatible generator), and offset blocks SHALL be displaced by vanilla's
position-derived XZ or XYZ offset.

#### Scenario: Seed values
- **WHEN** the seed for known positions is computed
- **THEN** it equals vanilla's hand-computed value

#### Scenario: Position-stable choice
- **WHEN** the same section is meshed twice
- **THEN** every random block picks the same variant

### Requirement: Partial-shape face culling
A face SHALL be emitted unless the neighbour's opposing face occlusion shape covers it, evaluated on the
actual face shapes so slabs, stairs and similar partial blocks cull like vanilla.

#### Scenario: Slab against slab
- **WHEN** two bottom slabs touch sideways
- **THEN** their shared side faces are not emitted

#### Scenario: Slab under a taller block
- **WHEN** a full block sits on a bottom slab
- **THEN** the slab's top face is emitted only over the uncovered area
