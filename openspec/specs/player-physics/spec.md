# player-physics Specification

## Purpose
Deterministic client-side simulation of vanilla player movement.

## Requirements

### Requirement: Tick simulation
The system SHALL advance a player by one game tick from an input state, a view rotation and a world, using the vanilla order of operations in double precision with single-precision casts where vanilla uses them, and SHALL return the resulting state flags.

#### Scenario: Jump apex
- **WHEN** a standing player on flat ground jumps and no other input is held
- **THEN** the highest position reached is within 1e-3 of 1.2522 above the start

#### Scenario: Terminal velocity
- **WHEN** a player falls in open air for 100 ticks
- **THEN** vertical velocity converges to about -3.92 blocks per tick

#### Scenario: Sprint jump boost
- **WHEN** a sprinting player jumps
- **THEN** horizontal velocity gains 0.2 along the facing direction using the 65536-entry sine table

### Requirement: Block collision
The system SHALL resolve movement against block collision shapes from game data, resolving Y first and then the larger of X and Z, with epsilon 1.0E-7, and SHALL step up obstacles no higher than the step height.

#### Scenario: Wall
- **WHEN** a player walks into a full block
- **THEN** it stops flush against the face and reports a horizontal collision

#### Scenario: Slab step
- **WHEN** a walking player meets a bottom slab
- **THEN** it ends the tick on top of the slab without jumping

#### Scenario: Sneaking at an edge
- **WHEN** a sneaking player on the ground moves toward a drop of more than the step height
- **THEN** the horizontal movement is reduced so it stays on the surface

### Requirement: Surface effects
The system SHALL apply block friction, speed factor and jump factor, slime and bed bounce, climbing, fluids, and stuck-in-block multipliers as vanilla does.

#### Scenario: Ice
- **WHEN** a player stops pressing keys on ice
- **THEN** it keeps sliding with horizontal damping of block friction times 0.91 per tick

#### Scenario: Water
- **WHEN** a player is in deep still water with no input
- **THEN** it sinks slowly with vertical damping 0.8 per tick

#### Scenario: Cobweb
- **WHEN** a player is inside a cobweb
- **THEN** the next move is scaled by (0.25, 0.05, 0.25) and fall distance resets

### Requirement: Creative flight
The system SHALL move flying players using the abilities flying speed and vertical input.

#### Scenario: Fly up
- **WHEN** a flying player holds jump
- **THEN** it gains vertical speed of three times the flying speed per tick before damping

### Requirement: Movement packets
The system SHALL decide, after each tick, whether to send a position, rotation, position and rotation, or status-only move packet using vanilla rules: position when squared displacement exceeds (2.0E-4)^2 or 20 ticks have passed since the last position, rotation on any change, status when ground or collision flags change.

#### Scenario: Idle player
- **WHEN** a player does not move for 19 ticks and then a 20th
- **THEN** a position packet is produced only on the 20th

#### Scenario: Rotation only
- **WHEN** only the yaw changes
- **THEN** a rotation packet is produced

### Requirement: Performance
A single tick in a dense collision area SHALL complete in well under 5 microseconds on a modern desktop core.

#### Scenario: Benchmark
- **WHEN** the tick benchmark runs
- **THEN** the mean time per tick is reported below 5 microseconds
