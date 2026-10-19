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

### Requirement: Eye and camera in fluid
The physics SHALL compute the eye-in-water state each tick like vanilla `EntityFluidInteraction`, using `getHeightForCamera` (full height for a source under a sturdy downward face, or for fluid with the same fluid above), and SHALL refresh it for passengers every tick. A shared camera check SHALL apply the same height rule to an arbitrary camera position.

#### Scenario: Full block under water
- **WHEN** the eye is in the top ninth of a water block that has water above
- **THEN** the eye is in water

#### Scenario: Riding
- **WHEN** the player rides a vehicle into water
- **THEN** its eye-in-water state follows the seat position

### Requirement: Creative flight with an elytra
A player that is creative flying SHALL NOT start gliding (`Player.canGlide` requires not flying), so double-tapping jump with an elytra equipped toggles creative flight like vanilla's `LocalPlayer.aiStep`. Every client-side change of the flying ability (double tap, landing, the spectator force) SHALL be reported with `ServerboundPlayerAbilities`.

#### Scenario: Double tap with an elytra
- **WHEN** a creative player wearing an elytra double-taps jump on the ground
- **THEN** creative flight turns on and no glide starts

### Requirement: Entity and world border colliders
Player and vehicle collision SHALL include the collision boxes of nearby entities that vanilla makes
collidable (boats, shulkers, happy ghasts) and the world border's collision shape when the player is
inside and close to the border.

#### Scenario: Standing on a boat
- **WHEN** the player falls onto a boat on land
- **THEN** the player lands on the boat's top and does not fall through

#### Scenario: Walking into the border
- **WHEN** the player walks toward the world border from inside
- **THEN** movement stops at the border plane without a server correction

### Requirement: Entity push and suffocation push-out
Remote pushable entities SHALL push the local player as `Entity#push` does, and a player whose corners
suffocate SHALL be nudged toward the closest free space as `LocalPlayer#moveTowardsClosestSpace` does.

#### Scenario: Crowd
- **WHEN** the player overlaps a stationary villager
- **THEN** the player's velocity gains a push away from the villager

### Requirement: Effect and equipment driven movement
Blindness SHALL prevent and stop sprinting, Dolphin's Grace SHALL use the 0.96 water slowdown, leather
boots in the feet slot SHALL make powder snow walkable and climbable, Weaving SHALL use the weaker cobweb
multiplier, and the scale attribute SHALL scale the player's box and eye height.

#### Scenario: Leather boots on powder snow
- **WHEN** a player wearing leather boots walks onto powder snow
- **THEN** the player stands on top of it

#### Scenario: Blind sprint
- **WHEN** the player has Blindness and double-taps forward
- **THEN** sprinting does not start and no START_SPRINTING is sent

### Requirement: Sanitised attributes
Attribute values used by physics SHALL be clamped to the attribute's vanilla range, and a NaN result SHALL
become the range minimum.

#### Scenario: Hostile step height
- **WHEN** the server sends a step_height modifier of 1e9
- **THEN** the step height used is 10

### Requirement: Vehicle parity
A saddled nautilus SHALL be steerable and send vehicle moves, a ridden pig or strider SHALL step at least
one block, horses and camels SHALL float while ridden in deep water, and turning a boat SHALL turn the
rider's view with the ±105° clamp.

#### Scenario: Pig step
- **WHEN** a player rides a pig with a carrot on a stick toward a one-block ledge
- **THEN** the pig steps up

### Requirement: Auto-jump
With `autoJump` on, walking into a one-block step SHALL trigger a jump as `LocalPlayer#updateAutoJump` does.

#### Scenario: Step ahead
- **WHEN** auto-jump is on and the player walks into a full block with air above it
- **THEN** the player jumps onto it
