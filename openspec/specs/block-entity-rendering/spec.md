# block-entity-rendering Specification

## Purpose
Draw block entities with model geometry, textures, animation and lighting in a caller's render pass.

## Requirements

### Requirement: Scene API
The system SHALL expose a `BlockEntityScene` that accepts entries (position, block state, kind, typed data, light), game time and partial ticks, and block event animation state, and renders them in a caller-provided pass with reverse-Z depth and camera-relative positions computed in double precision.

#### Scenario: Far-away block entities
- **WHEN** entries lie millions of blocks from the origin
- **THEN** they are placed relative to the camera in double precision

### Requirement: Container and furniture kinds
The system SHALL draw chests (single, double halves, trapped, ender, copper, christmas) with an animated lid, shulker boxes with colour and animated lid, bells with swing, lectern and enchanting table books with open and page-flip animation, decorated pots with sherd faces, conduits in closed and active forms, and copper golem statues.

#### Scenario: Double chest half open
- **WHEN** a left and right double chest half have an openness of 0.5
- **THEN** both lids are raised by the vanilla eased angle

### Requirement: Banners and heads
The system SHALL draw banners with base colour and up to 16 dyed pattern layers on the wall or standing model with a waving flag, and every skull type, using the default skin for player heads without a skin.

#### Scenario: Patterned banner
- **WHEN** a banner has a red base and two pattern layers
- **THEN** the flag shows the base colour with both layers over it in order

### Requirement: Display kinds
The system SHALL draw campfire items, shelf items, vault display items, brushable block items, moving piston blocks and the spinning entity of spawners and trial spawners through the entity scene.

#### Scenario: Spawner with pig
- **WHEN** a spawner entry names `minecraft:pig`
- **THEN** a pig spins above the spawner scaled to 0.53125

### Requirement: Beams and portals
The system SHALL draw beacon beams in coloured sections with scrolling texture and solid plus glow shells, end gateway beams, and the layered end portal and gateway shader on the visible cube faces.

#### Scenario: Coloured glass
- **WHEN** a beacon has a red then blue section
- **THEN** the lower beam is red and the upper blue

### Requirement: Sign text
The system SHALL draw both sides of signs and hanging signs in world space using vanilla font metrics, dye colours, glowing text with outline and the 16 block outline distance, and SHALL not draw text for signs beyond their view distance.

#### Scenario: Glowing sign
- **WHEN** a sign has glowing orange text
- **THEN** the text is fullbright and uses the dye colour with a dark outline when the camera is within 16 blocks

### Requirement: Performance
The system SHALL batch entries so that thousands of block entities cost a bounded number of draw calls.

#### Scenario: Five thousand entries
- **WHEN** 5000 chests are prepared and drawn
- **THEN** the draw call count is bounded by the number of distinct model and texture batches
