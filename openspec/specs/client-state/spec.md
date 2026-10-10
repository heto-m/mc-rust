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
