## ADDED Requirements

### Requirement: Entities are drawn from client state
The app SHALL draw every loaded entity within the entity render distance and the view frustum, interpolated between ticks, and SHALL skip entities outside loaded chunks.

#### Scenario: Entity outside range
- **WHEN** an entity is farther than the entity render distance
- **THEN** it is not submitted to the scene

#### Scenario: Interpolated position
- **WHEN** a frame is drawn at partial tick 0.5
- **THEN** the entity is placed midway between its previous and current tick positions

### Requirement: Name tags follow vanilla visibility
Name tags SHALL show for players and for named entities with a visible custom name, within 64 blocks (32 when sneaking), never for the local player in first person, and SHALL honour team name-tag visibility.

#### Scenario: Hidden by team
- **WHEN** the entity's team hides name tags
- **THEN** no tag is drawn

### Requirement: First-person hand
In first person the app SHALL draw the held item or the player's arm with a separate projection and cleared depth, with equip, swing and use animations.

#### Scenario: Slot change
- **WHEN** the selected hotbar item changes
- **THEN** the hand lowers and raises the new item over several ticks

### Requirement: Inventory preview
The inventory screen SHALL render the local player in its preview rectangle, turning toward the mouse.

#### Scenario: Preview present
- **WHEN** the player inventory is open
- **THEN** the player model is drawn inside the screen's entity rectangle

### Requirement: World effects are wired
The app SHALL configure the environment per dimension, clouds from options, weather while raining, destroy decals and particles from network events.

#### Scenario: Rain
- **WHEN** the server starts rain in the overworld
- **THEN** precipitation is drawn around the camera
