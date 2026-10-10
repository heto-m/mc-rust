## ADDED Requirements

### Requirement: Block entities
The app SHALL create block entities from chunk data and `block_entity_data`, remove them with their block or chunk, and drive lid and bell animation from `block_event`.

#### Scenario: Chunk unload
- **WHEN** a chunk unloads
- **THEN** its block entities are removed from the scene

### Requirement: Maps
The app SHALL decode `map_item_data` into bounded 128x128 map textures and draw them held, in item frames and as GUI items.

#### Scenario: Hostile patch
- **WHEN** a map patch exceeds the 128x128 area
- **THEN** it is clamped and no panic occurs

### Requirement: Translucent ordering
The app SHALL draw opaque entity parts before translucent terrain and blended parts after it, applying fog.

#### Scenario: Entity behind water
- **WHEN** an entity is below a water surface
- **THEN** the water tints it

### Requirement: Death screen lifetime
The app SHALL close the death screen when the local player is alive again.

#### Scenario: Server heals
- **WHEN** health above zero arrives while the death screen is open
- **THEN** the death screen closes
