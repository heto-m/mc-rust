## ADDED Requirements

### Requirement: View at pitch limits
The client SHALL clamp pitch to [-90, 90] degrees and SHALL render a consistent, unflipped view at the limits.

#### Scenario: Looking straight down
- **WHEN** the pitch is 90 degrees down at any yaw
- **THEN** screen-up points along the facing direction and screen-right is unchanged from the level view

### Requirement: Client-side light
The client SHALL recompute sky and block light locally when a block changes and SHALL remesh every section whose light or geometry changed, including neighbours; entities SHALL be lit from the light at their eye position.

#### Scenario: Broken block
- **WHEN** a block is broken and the opening exposes the area to sky or a light source
- **THEN** light in the opening updates immediately without waiting for the server

#### Scenario: Roof removed
- **WHEN** the only block roofing a column is removed
- **THEN** every cell in that column reaches full sky light and neighbouring cells fall off by one per block

### Requirement: Local break particles
The client SHALL spawn destroy particles when the local player's own break removes a block, since the server omits the breaker from the broadcast.

#### Scenario: Own break
- **WHEN** the local player breaks a block
- **THEN** destroy particles appear at the block

### Requirement: Entity picking
The client SHALL target only pickable entities (not items, experience orbs, arrows, area effect clouds, markers, display entities or spectators), SHALL widen redirectable projectiles by one block, and SHALL choose an entity only when strictly nearer than the block hit.

#### Scenario: Dropped item
- **WHEN** the crosshair is over a dropped item
- **THEN** the item is not targeted and use actions go to the block behind it

### Requirement: Click consumption and block placement
The client SHALL act on every use and attack click even when the key is released before the next tick, and SHALL predict block placement locally.

#### Scenario: Quick click
- **WHEN** the use key is pressed and released within one tick while looking at a block with a block item held
- **THEN** the block is placed and shown at once

#### Scenario: Replaceable target
- **WHEN** a block is used against tall grass
- **THEN** the grass is replaced by the held block

### Requirement: Idle stability
A stationary player on leaves or in still water SHALL not drift horizontally, and the client SHALL send position packets only at the vanilla cadence.

#### Scenario: Standing on leaves
- **WHEN** the player stands with no input for 30 seconds on leaves
- **THEN** the position is unchanged and no server correction occurs
