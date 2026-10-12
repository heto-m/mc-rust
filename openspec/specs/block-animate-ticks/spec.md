# block-animate-ticks Specification

## Purpose
Client-side ambient block effects: the particles and sounds vanilla's `animateTick` makes around the player.

## Requirements

### Requirement: Sampling
Each client tick the client SHALL sample 667 random block positions within 16 blocks and 667 within 32 blocks of the player's block, using `offset = nextInt(r) - nextInt(r)` per axis, and run the ambient effects of the block, of the fluid in it and of the biome's ambient particles at each. The work per tick SHALL NOT depend on the world contents.

#### Scenario: Fixed work
- **WHEN** every sampled block carries the most expensive behaviour
- **THEN** exactly 1334 positions are visited and the particles spawned stay within the per-tick budget

### Requirement: Block effects
The client SHALL reproduce the particles and sounds of every vanilla 26.3 block with an `animateTick` override, with the vanilla probabilities, offsets and velocities.

#### Scenario: Torch
- **WHEN** a torch is sampled
- **THEN** one smoke and one flame particle spawn at 0.5, 0.7, 0.5 of the block

#### Scenario: Lit furnace
- **WHEN** a lit furnace is sampled
- **THEN** smoke and flame spawn on its front face and the crackle sound plays with probability 0.1

#### Scenario: Unlit furnace
- **WHEN** an unlit furnace is sampled
- **THEN** nothing spawns

#### Scenario: Leaves over air
- **WHEN** oak leaves with a non-full top below are sampled
- **THEN** a tinted leaf particle spawns below the block with probability 0.01

### Requirement: Fluid effects
A sampled lava block with air above SHALL pop with probability 1/100 (lava particle and pop sound) and play its ambient sound with probability 1/200. A flowing water block SHALL play the water ambient sound with probability 1/64, and water source or falling water SHALL spawn an underwater particle with probability 1/10. Any fluid with a drip particle SHALL, with probability 1/10, drip from the face below the block when that face leaks.

#### Scenario: Lava under a ceiling
- **WHEN** lava with a solid block above is sampled
- **THEN** no pop or ambient sound plays

### Requirement: Biome ambient particles
For sampled positions whose block has no full collision shape, the client SHALL roll each particle of the position's `minecraft:visual/ambient_particles` attribute (the dimension type's list followed by the biome's appended entries) with `nextFloat() <= probability` and spawn it at a random point of the block. Lists are capped at 16 entries and probabilities are clamped to 0 to 1.

#### Scenario: Soul sand valley
- **WHEN** air in a soul sand valley is sampled
- **THEN** an ash particle spawns with probability 0.00625

#### Scenario: Hostile list
- **WHEN** a biome lists thousands of particles with probability 1
- **THEN** at most 16 are used and the per-tick budget still holds

### Requirement: Particles option
Particles from the scan SHALL follow the Particles option: all shows everything, decreased drops a third, minimal drops everything except always-visible particles that the one-in-ten upgrade lets through. Particles farther than 32 blocks from the camera are skipped unless their type overrides the limiter.

#### Scenario: Minimal
- **WHEN** the option is minimal
- **THEN** a torch spawns no smoke or flame

### Requirement: Campfire smoke
Each loaded lit campfire SHALL, with probability 0.11 per tick, spawn two to three rising smoke particles, signal smoke when the signal fire property is set.

#### Scenario: Signal fire
- **WHEN** a campfire above hay emits smoke
- **THEN** the particles are `campfire_signal_smoke`

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
