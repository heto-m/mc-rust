## MODIFIED Requirements

### Requirement: Shared camera and depth
Java's camera and level projection SHALL equal mcr's render camera for each frame, and Java world-space draws SHALL depth-test against mcr's reverse-Z scene depth. mcr's render camera SHALL be its own player camera unless a Java camera handoff is active.

#### Scenario: Occlusion by terrain
- **WHEN** a zombie stands behind a stone wall relative to the camera
- **THEN** no zombie pixels are visible

## ADDED Requirements

### Requirement: Camera handoff
When a Java mod makes Java's final camera differ from mcr's player camera, by moving or rotating it or by making another entity the camera entity, Java SHALL report that camera. While Java draws the world, mcr SHALL render the next frame's terrain from the reported camera, and Java SHALL render that frame from the same camera. When no mod moves the camera, frames SHALL be identical to frames without this feature.

#### Scenario: Camera moved by a mixin
- **WHEN** a mod moves Java's camera three blocks up at the end of its camera update
- **THEN** mcr renders terrain from three blocks above the player's eye, and the player does not move

#### Scenario: Mod releases the camera
- **WHEN** the mod makes the player the camera entity again
- **THEN** mcr renders from its own player camera

### Requirement: Handoff camera bounds
mcr SHALL ignore a reported camera with non-finite values, and SHALL clamp its position to the loaded world plus a margin.

#### Scenario: Hostile camera
- **WHEN** Java reports a camera with a NaN coordinate or one far outside the loaded chunks
- **THEN** mcr ignores the NaN report, or clamps the position to the loaded area plus the margin

### Requirement: Handoff input capture
Java SHALL report whether the mod took the player's movement input. While it has, mcr's player SHALL neither move nor turn, and Java SHALL receive the movement keys and mouse look.

#### Scenario: Free camera
- **WHEN** a mod makes another entity the camera entity and takes the player's movement input, and the user holds forward
- **THEN** mcr's camera moves forward with that entity, and mcr's player stays where it was

#### Scenario: Input returned
- **WHEN** the mod gives the player its movement input back
- **THEN** movement keys move mcr's player again

### Requirement: Weather replacement
When a mod cancels or redirects vanilla's weather extraction in Java, Java SHALL report it for the frame, and mcr SHALL then not draw its own rain and snow while Java draws the world.

#### Scenario: Weather mod
- **WHEN** a mod cancels `WeatherEffectRenderer.extractRenderState` while it rains
- **THEN** mcr draws no rain or snow columns, and mcr still draws its sky and fog for the rain

#### Scenario: No weather mod
- **WHEN** no mod touches weather rendering and it rains
- **THEN** mcr draws its rain as before

### Requirement: Section visibility for Java culling
mcr SHALL share the set of sections its visibility graph reached for the current render camera with Java. Java SHALL skip the renderable block entities of sections outside that set, and SHALL skip entities whose bounding boxes touch only sections outside it. Sections outside the shared area, and frames without a set, SHALL be culled as before.

#### Scenario: Mobs sealed in stone
- **WHEN** 500 zombies stand in an air pocket enclosed by solid stone sections, out of the camera's sight
- **THEN** Java extracts none of them, and its frame time drops accordingly

#### Scenario: Large entity across sections
- **WHEN** an entity's position lies in an unreached section but its bounding box reaches into a reached one
- **THEN** Java still draws it
