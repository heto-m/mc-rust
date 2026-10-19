## ADDED Requirements

### Requirement: Sleeping
While the server reports the local player sleeping, the client SHALL block movement input, use the
sleeping box and bed camera, hide the first-person hand, fade the screen and show the in-bed chat screen
whose Leave Bed button and Esc send STOP_SLEEPING.

#### Scenario: Leave bed
- **WHEN** the player lies in a bed at night and presses Leave Bed
- **THEN** a STOP_SLEEPING player command is sent and the screen closes

### Requirement: Weapon and item use parity
Left-click with a piercing weapon SHALL send the STAB action instead of an attack or a dig. While the
server reports the player using an item, releasing the use key SHALL send RELEASE_USE_ITEM even when the
client did not predict the use, and attack, dig, repeated use and pick clicks SHALL be ignored.

#### Scenario: Bundle
- **WHEN** the player holds use on a bundle and releases it
- **THEN** RELEASE_USE_ITEM is sent and the bundle is not emptied by one click

#### Scenario: Shield up
- **WHEN** the player blocks with a shield and clicks attack
- **THEN** no attack packet is sent

### Requirement: Interaction ranges and targets
Entity and block targeting SHALL use the synced `entity_interaction_range` and `block_interaction_range`
attributes and the entity's current dimensions, and an entity interaction SHALL send the hit location
relative to the entity.

#### Scenario: Survival entity reach
- **WHEN** a survival player looks at a mob 4 blocks away with default attributes
- **THEN** the mob is not targeted

#### Scenario: Armour stand
- **WHEN** the player right-clicks an armour stand's helmet with an empty hand
- **THEN** the interact packet's location has the helmet's height

### Requirement: Adventure mode predicates
In adventure mode the client SHALL allow digging when the held item's `can_break` matches the block and
placement when its `can_place_on` matches, and SHALL refuse both otherwise.

#### Scenario: can_break pickaxe
- **WHEN** an adventure player holds a pickaxe whose can_break matches stone and digs stone
- **THEN** START_DESTROY_BLOCK is sent

### Requirement: Per-tick mining
Survival digging SHALL accumulate destroy progress every tick from the current state (ground, fluid,
effects, tool) and restart when the target or held item changes, with a 5-tick delay between blocks.

#### Scenario: Jump mid-dig
- **WHEN** the player jumps halfway through digging a block
- **THEN** the remaining time grows by the off-ground factor
