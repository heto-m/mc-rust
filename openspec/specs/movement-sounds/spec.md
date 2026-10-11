# movement-sounds Specification

## Purpose
Local player footsteps, swimming, splash and landing sounds from block sound types.

## Requirements

### Requirement: Footsteps
The local player SHALL step every time the accumulated horizontal distance times 0.6 passes the next whole step, playing the supporting block's step sound at 15% of its type volume and its type pitch, with the block above substituted for inside-step blocks and layered for combination blocks.

#### Scenario: Walking on stone
- **WHEN** the player walks on stone
- **THEN** `block.stone.step` plays at volume 0.15 about every 1.67 blocks

#### Scenario: Sneaking and flying
- **WHEN** the player sneaks on the ground or flies
- **THEN** no step sounds play

#### Scenario: Ladders
- **WHEN** the player climbs a climbable block
- **THEN** steps are counted from total movement

### Requirement: Swimming and splashing
The local player SHALL play the swim sound at the step cadence while in water, together with a muffled step, and a splash sound when entering water whose variant depends on entry speed.

#### Scenario: Fast entry
- **WHEN** the player enters water quickly
- **THEN** the high-speed splash plays

### Requirement: Landing
When the local player lands from a fall that would deal damage, the client SHALL play the small (up to 4 damage) or big player fall sound and the block's fall sound at half volume and 0.75 pitch.

#### Scenario: Safe fall
- **WHEN** the fall distance is 3 or less
- **THEN** no landing sound plays

#### Scenario: Soft block
- **WHEN** the player lands on hay
- **THEN** damage is computed with a 0.2 modifier

### Requirement: Block sound types
Step, fall, place, hit and break events SHALL derive from the block's registered sound type, including its volume and pitch.

#### Scenario: Metal
- **WHEN** the block sound type is metal
- **THEN** its pitch is 1.5
