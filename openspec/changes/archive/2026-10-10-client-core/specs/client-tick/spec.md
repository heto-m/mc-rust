## ADDED Requirements

### Requirement: Fixed-step tick
The client SHALL advance in 20 TPS steps: process inbound packets, simulate the local player, send movement packets and `client_tick_end`.

#### Scenario: Tick order
- **WHEN** `Client::tick` runs in play
- **THEN** inbound packets are applied before physics, and `client_tick_end` is sent last

#### Scenario: Player loaded
- **WHEN** terrain under the player first becomes available
- **THEN** `player_loaded` is sent once

### Requirement: Chunk batch pacing
The client SHALL answer each finished chunk batch with a desired chunks-per-tick value computed from observed batch durations.

#### Scenario: Batch reply
- **WHEN** a batch finishes
- **THEN** `chunk_batch_received` is sent with a positive finite rate

### Requirement: Actions
The client SHALL expose chat, command, swing, use, dig, place, attack, interact, hotbar, container and respawn actions that emit the vanilla packets.

#### Scenario: Offline chat
- **WHEN** a chat message is sent without profile keys
- **THEN** an unsigned `chat` packet is emitted

#### Scenario: Digging
- **WHEN** a block is broken
- **THEN** start and stop `player_action` packets carry increasing sequence ids

### Requirement: Events
The client SHALL queue `ClientEvent`s for the renderer and UI to drain.

#### Scenario: Drain
- **WHEN** the caller drains events
- **THEN** all events since the last drain are returned in order and the queue is empty
