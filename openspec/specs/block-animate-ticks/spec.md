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
