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
