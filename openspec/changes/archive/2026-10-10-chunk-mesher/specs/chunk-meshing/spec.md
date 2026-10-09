# Spec Delta

## Purpose

Defines how chunk sections become packed render geometry.

## ADDED Requirements

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
Each cube face corner SHALL get a 2-bit occlusion level from the two side blocks and the corner block
using vanilla's rule, and quads whose corners are not constant along a merge direction SHALL NOT merge
along it.

#### Scenario: Flat floor
- **WHEN** a face has no occluding neighbours
- **THEN** all four corners have the brightest level

#### Scenario: Concave corner
- **WHEN** both side neighbours of a corner occlude
- **THEN** that corner has the darkest level

### Requirement: Fluids
Water and lava SHALL be meshed with corner heights derived from fluid levels and neighbours, and water
SHALL go to the translucent layer.

#### Scenario: Source under air
- **WHEN** a water source has air above
- **THEN** its top face is lower than one block

### Requirement: Biome tints
Grass, foliage, dry foliage and water tints SHALL be averaged over a configurable biome blend radius
(default 2) and selected per quad by tint kind.

#### Scenario: Blend radius zero
- **WHEN** the radius is 0
- **THEN** each column uses its own biome colour

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
