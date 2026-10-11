## ADDED Requirements

### Requirement: Damage effects
A `damage_event` SHALL resolve its damage type's `effects` from the synced `damage_type` registry, falling back to vanilla's built-in types by name when the entry carries no data, and SHALL queue them with the damage event so the local player's hurt sound matches (`hurt`, `hurt_on_fire`, `hurt_drown`, `hurt_sweet_berry_bush`, `hurt_freeze`).

#### Scenario: Burning
- **WHEN** the local player takes `lava`, `in_fire` or `on_fire` damage
- **THEN** the queued effects select `entity.player.hurt_on_fire`

#### Scenario: Custom damage type
- **WHEN** a server-defined damage type declares `effects: freezing`
- **THEN** the queued effects select `entity.player.hurt_freeze`

### Requirement: Item pickup animation
A `take_item_entity` SHALL add a pickup that flies the collected entity, as it was before the stack shrank, to the collector's mid-body over 3 ticks with vanilla `ItemPickupParticle` interpolation, and SHALL remove the item entity when its stack empties. Pickups SHALL be dropped when the world is rebuilt or reconfigured.

#### Scenario: Whole stack collected
- **WHEN** a player collects a whole item stack
- **THEN** the item entity is removed at once and its pickup reaches the collector after 3 ticks and disappears

#### Scenario: Part of a stack collected
- **WHEN** only part of a stack is collected
- **THEN** the item entity stays with the remaining count and a pickup still flies to the collector
