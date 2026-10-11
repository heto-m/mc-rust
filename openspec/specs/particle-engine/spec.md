# particle-engine Specification

## Purpose
Defines the CPU-simulated, GPU-instanced particle engine, its hard caps and the vanilla emit semantics.

## Requirements

### Requirement: Bounded simulation
The engine SHALL cap live particles at 16384 per render type and clamp the count of any single emit request.

#### Scenario: Flood
- **WHEN** a packet requests more particles than the cap
- **THEN** only the available capacity is spawned

### Requirement: Level particle semantics
`emit` SHALL spread particles by the offsets when the count is positive and treat the offsets as a direction scaled by the speed when the count is zero.

#### Scenario: Directional
- **WHEN** count is zero
- **THEN** one particle spawns at the position with velocity offset times speed

### Requirement: Block particles
`block_break` and `block_crack` SHALL spawn terrain particles that sample the block texture sub-UV region.

#### Scenario: Break
- **WHEN** a block is broken
- **THEN** a 4x4x4 grid of terrain particles spawns inside the block

### Requirement: World interaction
Particles that collide SHALL stop against solid boxes returned by a collision callback and take light from a light callback.

#### Scenario: Floor
- **WHEN** a falling particle meets a solid box
- **THEN** its vertical velocity is zeroed and friction applies

### Requirement: Draw
Live particles SHALL be drawn as instanced camera-facing billboards.

#### Scenario: Throughput
- **WHEN** 10000 particles are alive
- **THEN** update and instance building take a few milliseconds at most

### Requirement: Firework sparks
The engine SHALL simulate firework sparks with vanilla's gravity, friction and lifetime, a spark colour that fades to an optional fade colour in the second half of life, trail copies spawned half-aged, and twinkling that hides the spark in alternating three-tick windows after the first third of its life. The firework flash SHALL grow and fade over four ticks.

#### Scenario: Trail
- **WHEN** a trailing spark ticks during the first half of its life
- **THEN** it leaves stationary half-aged copies of itself

#### Scenario: Flash
- **WHEN** a firework flash is added
- **THEN** it is drawn for four ticks with a size following vanilla's sine curve and then removed
