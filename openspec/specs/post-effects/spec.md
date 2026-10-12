# post-effects Specification

## Purpose
Vanilla post chains over the world image, run before the UI: the chains the server requests, the chain for the spectated entity, and glowing entity outlines.

## Requirements

### Requirement: Requested post chains
The client SHALL request, in order:
- the chains of the server's latest `post_effects` list;
- then the spectated-entity chain while it is active.

Only `minecraft:blur`, `minecraft:creeper`, `minecraft:invert` and `minecraft:spider` SHALL be applied. Any other id SHALL be ignored. At most 8 chains SHALL run per frame. The server list SHALL be kept to at most 16 ids, SHALL be ignored outside the play phase and SHALL be cleared on respawn.

#### Scenario: Unknown id
- **WHEN** the server sends `minecraft:entity_outline`, `foo:bar` and `minecraft:invert`
- **THEN** only the invert chain runs

#### Scenario: Respawn
- **WHEN** the player respawns
- **THEN** no server chain runs until the server sends the list again

### Requirement: Spectated entity chain
When the camera entity changes, the spectated chain SHALL become `creeper` for a creeper, `spider` for a spider or cave spider, `invert` for an enderman, and none otherwise, and it SHALL be active. Leaving first person SHALL clear it. Returning to first person SHALL set it again from the camera entity. `key.toggleSpectatorShaderEffects` SHALL toggle whether it is active.

#### Scenario: Spectating a creeper
- **WHEN** the server sets the camera to a creeper while in first person
- **THEN** the creeper chain runs until the camera returns to the player

### Requirement: Post chain passes
Each chain SHALL be a sequence of fullscreen passes over the main image and pooled screen-sized targets. Each pass SHALL read only targets that are already written, SHALL never read the target it writes, and the chain's last pass SHALL write the main image. Sampling SHALL use vanilla's bottom-up texture coordinates, so asymmetric passes such as the spider eyes and the mosaic grid keep vanilla's orientation.

#### Scenario: Invert
- **WHEN** the invert chain processes a uniform colour `c`
- **THEN** the result is `c + 0.8 * (1 - 2c)` per channel

#### Scenario: Creeper
- **WHEN** the creeper chain processes a uniform colour
- **THEN** red and blue are zero and green is the saturated, 16-level quantised luma

### Requirement: Post chains before the UI in every present mode
The renderer SHALL run the requested chains after the world, the world-space overlays and the hand, and before UI overlays. This SHALL hold for offscreen targets, window surfaces and scene textures. A window surface without a scene texture SHALL render through an intermediate image only while a chain is requested. With an external submitter, the chains SHALL run after `after_world` and before `before_ui`.

#### Scenario: Offscreen invert
- **WHEN** an offscreen frame of one known colour is rendered with the invert chain
- **THEN** the read-back pixels hold the inverted colour

### Requirement: Glowing entity outlines
A natively drawn entity whose glowing shared flag is set SHALL get an outline in its team colour, or in white without a team colour. A spectator holding `key.spectatorOutlines` SHALL see every player outlined. The outline SHALL be the edge of the entity's silhouette, widened by vanilla's `entity_outline` passes. It SHALL show through other geometry, and it SHALL blend over the world after the hand and before the screen chains.

#### Scenario: Silhouette edge
- **WHEN** a glowing silhouette covers a square
- **THEN** pixels at the square's edge take its colour, and pixels well inside or outside it are unchanged

### Requirement: Mod mode ownership
In mod mode, Java SHALL skip vanilla post chains whose id mcr applies, and SHALL still apply other chains. Java's GUI slot SHALL be submitted in `before_ui`, after mcr's chains.

#### Scenario: Spectating in mod mode
- **WHEN** Java's world is composed and the camera spectates an enderman
- **THEN** the invert chain runs once, after Java's hand and before Java's GUI

### Requirement: Resource-pack post chains
A resource pack's `assets/minecraft/post_effect/<chain>.json` for `blur`, `creeper`, `invert` or `spider` SHALL retune that chain only when it has the built-in chain's shape. Any other file, including one with pack shaders, SHALL leave the built-in chain unchanged and a warning SHALL be logged. `entity_outline` SHALL always stay built in.

#### Scenario: Retuned invert
- **WHEN** a pack's `invert.json` sets `InverseAmount` to 0.25 and keeps the rest
- **THEN** the invert chain inverts by 0.25

#### Scenario: Custom shader
- **WHEN** a pack's chain names `pack:post/custom` as a fragment shader
- **THEN** the built-in chain runs and a warning is logged

### Requirement: Pack chain shape
A pack chain SHALL have the same number of passes as the built-in chain, the same vanilla fragment and vertex shader ids, the same input and output targets and filters, only empty target definitions, and every uniform each pass reads.

#### Scenario: Different graph
- **WHEN** a pack chain writes a pass to another target or defines a sized target
- **THEN** it is ignored

### Requirement: Pack chain values
Uniform values read from a pack chain SHALL be clamped to a safe range per uniform, and a pack chain file larger than 64 KiB SHALL be ignored.

#### Scenario: Wild value
- **WHEN** `InverseAmount` is `1e300`
- **THEN** the amount used is 2
