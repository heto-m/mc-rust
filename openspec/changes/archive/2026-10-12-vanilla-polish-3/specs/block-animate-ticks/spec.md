## ADDED Requirements

### Requirement: Creative block markers
In creative mode with a barrier or a light block in the main hand, the scan SHALL emit a `block_marker` particle at the centre of every sampled block state of that block.

#### Scenario: Holding a barrier
- **WHEN** a creative player holds a barrier and a barrier is sampled
- **THEN** a block marker spawns at the centre of the block

#### Scenario: Survival
- **WHEN** the player is not in creative mode
- **THEN** no marker spawns

### Requirement: Campfire food smoke
A lit campfire SHALL emit four smoke particles with velocity (0, 0.0005, 0) at the slot's corner, 20% per tick for every filled item slot, with the slot positions rotated by the facing.

#### Scenario: One item
- **WHEN** a north-facing campfire holds one item in slot 0
- **THEN** its smoke starts at x + 0.8125 and z + 0.8125 of the block

### Requirement: Spawner client tick
A spawner with a mob set and a player within its required range SHALL emit smoke and a flame at one random point of the block each tick, and its cage SHALL spin only then.

#### Scenario: No player
- **WHEN** no player is in range
- **THEN** no particles spawn and the spin stops

### Requirement: Trial spawner client tick
A trial spawner SHALL emit particles by its state (small flames waiting, flames and smoke active, cooldown smoke with a burst on the top face each second; soul fire when ominous) and play its ambient sound at 2% per tick while it can spawn. Its mob SHALL spin at 200 (waiting) or 1000 (active) over the time to the next spawn plus 200.

#### Scenario: Inactive
- **WHEN** the state is inactive
- **THEN** no particles spawn

### Requirement: Vault client tick
A vault SHALL emit 2 to 5 `vault_connection` particles per linked player in range from its keyhole once a second, smoke at 50% per tick (with small flames, or soul fire flames when ominous, while it shows an item), and its ambient sound at 2% per tick while it shows an item.

#### Scenario: Linked player
- **WHEN** a linked player is within range on the tick that divides 20
- **THEN** between 2 and 5 connection particles spawn at the keyhole

### Requirement: Conduit client tick
A conduit SHALL emit a nautilus particle from above its block toward a frame block with probability 1 in 50 per block per tick, and one at its attack target each tick.

#### Scenario: Target
- **WHEN** a conduit has a target
- **THEN** a nautilus particle spawns at the target's eye every tick
