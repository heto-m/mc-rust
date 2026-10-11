## ADDED Requirements

### Requirement: Entity culling
The app SHALL skip building and drawing any entity or flying item pickup whose position is non-finite, farther than the mean bounding-box edge x 64 x the entity distance scaling, or whose bounding box inflated by 0.5 blocks lies outside the view frustum.

#### Scenario: Large entity keeps its range
- **WHEN** a 4x4 block entity is 200 blocks away with scaling 1
- **THEN** it is drawn, while a 0.6x1.8 entity at 70 blocks is not

#### Scenario: Pickup behind the camera
- **WHEN** an item flying to its collector is behind the camera
- **THEN** it is not drawn

### Requirement: Block entity culling
The system SHALL cull each block entity against its per-kind view distance (64 blocks, 68 for pistons, 256 for end gateways, the render distance for beacons) and its render box, which is the block inflated by 0.5 and unbounded in height for beacons and end gateways, measuring beam kinds by horizontal distance only.

#### Scenario: Beam from far above
- **WHEN** a beacon lies 300 blocks above the view frustum within the render distance
- **THEN** it is still drawn, while a chest at the same height is not

### Requirement: Fog distance culling of sections
The renderer SHALL skip opaque and translucent draws of sections whose nearest point is at or beyond the render-distance fog end, measured as the larger of horizontal distance and vertical offset, and SHALL NOT cull when that end is not positive.

#### Scenario: Corner past the fog
- **WHEN** a section's nearest point lies 101.8 blocks away diagonally and the fog end is 100
- **THEN** the section produces no draws although it is inside the loaded square

#### Scenario: Nothing changes inside the fog
- **WHEN** the same scene is rendered with and without fog culling and no section reaches the fog end
- **THEN** the frames are identical
