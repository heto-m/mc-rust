## ADDED Requirements

### Requirement: Block marker particles
A `block_marker` particle SHALL draw the block's whole particle sprite at 0.5 blocks without tint or motion for 80 ticks, and SHALL be skipped when the block has no particle sprite.

#### Scenario: Lifetime
- **WHEN** a block marker spawns
- **THEN** it exists for 81 ticks

### Requirement: Fly-towards particles
`enchant`, `nautilus` and `vault_connection` particles SHALL start offset by their velocity from the spawn point and fly back to it over 30 to 39 ticks, dipping by 1.2 blocks at the start. `vault_connection` SHALL be translucent, fade in from a quarter of its life and be lit at full block light.

#### Scenario: Flight
- **WHEN** an enchant particle spawns at (10, 20, 30) with velocity (2, 0, -1)
- **THEN** it starts at (12, 20, 29) and ends at x 10 and y 18.8
