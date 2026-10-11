# client-state Specification

## Purpose
Client-side game state (world, entities, local player, inventory, social and UI state) built from clientbound packets.

## Requirements

### Requirement: World state
The client SHALL keep a chunk store, dimension parameters, time, weather, border and spawn derived from clientbound packets.

#### Scenario: Chunk received
- **WHEN** a chunk with light arrives
- **THEN** it is stored in the chunk store and a chunk-loaded event is queued

#### Scenario: Block update
- **WHEN** a block or section update arrives for a loaded chunk
- **THEN** the store reflects it and the touched sections become dirty

#### Scenario: Malformed chunk
- **WHEN** a chunk payload fails to decode
- **THEN** it is dropped without panicking and the connection stays up

### Requirement: Entities
The client SHALL track entities with position, rotation, velocity, metadata, equipment, passengers, effects and attributes.

#### Scenario: Relative move
- **WHEN** a relative move arrives
- **THEN** the target position is the previous position plus the decoded delta

#### Scenario: Removal
- **WHEN** entities are removed
- **THEN** they disappear from state and removal events are queued

### Requirement: Local player and inventory
The client SHALL track the local player's position, vitals, abilities, game mode and inventory with container state ids.

#### Scenario: Server teleport
- **WHEN** `player_position` arrives
- **THEN** relative flags are resolved against the current state, physics snaps to the result and `accept_teleportation` is sent

#### Scenario: Container sync
- **WHEN** container content or slot packets arrive
- **THEN** slots, cursor and state id are updated

### Requirement: Social and UI state
The client SHALL keep chat history, player list, titles, boss bars and scoreboard state.

#### Scenario: Chat
- **WHEN** a system, player or disguised chat message arrives
- **THEN** it is appended to bounded history and a chat event is queued

### Requirement: Editor and dialog events
The client SHALL queue events for opening block and minecart editors, and for the test instance status packet, and SHALL forward `show_dialog` and `clear_dialog` from both configuration and play to a dialog channel, resolving registry references to dialog compounds with at most 64 sub-dialogs, and send the answers it receives from that channel as `custom_click_action` or commands.

#### Scenario: Registry reference
- **WHEN** `show_dialog` names a registry id
- **THEN** the channel receives the registry's dialog compound

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

### Requirement: Entity event state
The client SHALL apply the state changes of vanilla's client `handleEntityEvent`: event 55 swaps another entity's main and off hand equipment, a ravager's event 69 knocks the local player back within 4 blocks of its box, a fishing hook's event 31 pulls a hooked local player toward the angler, events 22 and 23 set reduced debug info, and event 9 ends the local player's item use.

#### Scenario: Ravager roar
- **WHEN** a ravager two blocks away roars
- **THEN** the local player's velocity gains vanilla's strong knockback away from it

#### Scenario: Out of range
- **WHEN** a ravager twenty blocks away roars
- **THEN** the local player's velocity does not change
