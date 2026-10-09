# game-data Specification

## Purpose
Version-aware access to static game facts needed by the client, generated from vanilla data.

## Requirements

### Requirement: Block state facts
The system SHALL expose, for every block state id in the vanilla block report, its block name, property values, collision and outline shapes as axis-aligned boxes, light emission, light dampening, occlusion flags, fluid state, and air/replaceable/block-entity flags.

#### Scenario: Every state is covered
- **WHEN** a state id from the block report is queried
- **THEN** a block state record is returned

#### Scenario: Known values
- **WHEN** glowstone, air and a bottom slab are queried
- **THEN** light emission is 15, air reports as air, and the slab collision box spans y 0 to 0.5

### Requirement: Registry lookups
The system SHALL map registry entry names to ids and back for blocks, items, entity types and fluids, and expose item and entity type facts.

#### Scenario: Name round trip
- **WHEN** a block name is resolved to a block and back
- **THEN** the original name is returned

### Requirement: Packet ids
The system SHALL resolve protocol packet ids by connection state, direction and packet name.

#### Scenario: Login finished
- **WHEN** the clientbound login packet `minecraft:login_finished` is queried
- **THEN** its id is 2

### Requirement: Deterministic generation
Regenerating tables from unchanged inputs SHALL produce byte-identical output.

#### Scenario: Repeat generation
- **WHEN** the generator runs twice on the same inputs
- **THEN** the outputs are identical
