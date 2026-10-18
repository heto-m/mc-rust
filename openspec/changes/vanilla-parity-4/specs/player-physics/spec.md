## ADDED Requirements

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
