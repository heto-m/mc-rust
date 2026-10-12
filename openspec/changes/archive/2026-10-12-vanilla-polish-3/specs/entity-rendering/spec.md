## ADDED Requirements

### Requirement: Texture layer reuse
The entity renderer SHALL return the texture-array layer of a replaced or evicted texture to its size class and SHALL NOT reuse it until the frames in flight have completed. Freeing a layer twice SHALL NOT make two textures share it.

#### Scenario: Replaced map
- **WHEN** a map image is replaced
- **THEN** its previous layer is freed and is reusable after four frames

#### Scenario: Freed layer still in flight
- **WHEN** a layer is freed and a texture is added in the same frame
- **THEN** the new texture gets a different layer

### Requirement: Bounded runtime textures
Skin and armor-trim textures composed at runtime SHALL be evicted when unused for 1800 frames, and SHALL be limited to 384 live textures by evicting the least recently used ones not used this frame. A texture a live entity uses SHALL NOT fall back to the missing texture because of eviction.

#### Scenario: Skin churn
- **WHEN** 10000 distinct skins are registered one per frame
- **THEN** at most 384 layers are live and each skin resolves to its own texture while in use

#### Scenario: Map table full
- **WHEN** a new map arrives while 256 maps are held
- **THEN** the least recently drawn map and its layer are released and the new map is kept

### Requirement: Display teleport duration
A display entity SHALL move to a new server position and rotation over its `teleport_duration` ticks (clamped to 0..59), covering `1 / remaining` of the way left each tick with the shortest-path yaw. With a duration of 0 it SHALL snap.

#### Scenario: Interpolated move
- **WHEN** a display with a teleport duration of 4 is told to move 8 blocks
- **THEN** it is 2, 4, 6 and 8 blocks along after four ticks

#### Scenario: Hostile duration
- **WHEN** the synced duration is `i32::MAX`
- **THEN** 59 ticks are used

### Requirement: Text display draw order
Text displays SHALL draw from the farthest to the nearest, each with its background and glyphs together, whether or not they are see-through.

#### Scenario: Overlapping displays
- **WHEN** two translucent text displays overlap on screen
- **THEN** the nearer one blends over the farther one
