# protocol-play-player Specification

## Purpose
Wire format of the player-facing play clientbound packets: player state, chat, inventory, scoreboard, UI, commands, recipes and advancements.

## Requirements

### Requirement: Player-facing play clientbound packets
The protocol crate SHALL provide a typed codec for each of the player, inventory, chat, scoreboard, UI, command, recipe and advancement play clientbound packets, byte-compatible with Minecraft 26.3.

#### Scenario: Round trip
- **WHEN** any such packet is encoded and decoded
- **THEN** the decoded value equals the original and no input remains

#### Scenario: Byte-exact fixtures
- **WHEN** a packet such as set_health or player_position is encoded
- **THEN** the bytes equal those the vanilla codec produces for the same field values

### Requirement: Command graph
The commands packet SHALL carry the full Brigadier node graph, including node flags, children, redirect, name, argument parser id with parser-specific properties for every argument type, and suggestion type.

#### Scenario: Argument properties
- **WHEN** a node uses a ranged numeric, string-kind, entity-selector or registry-bound argument
- **THEN** its flags and bounds are decoded and re-encoded unchanged

### Requirement: Polymorphic displays
Slot displays and recipe displays SHALL decode and encode through their registry-id-tagged variants, including nested displays.

#### Scenario: Nested slot display
- **WHEN** a smithing recipe display containing composite slot displays is round-tripped
- **THEN** the structure is preserved

### Requirement: Malformed input
Decoding SHALL return an error rather than panic on unknown variant ids, oversized lengths or truncated input.

#### Scenario: Unknown recipe display id
- **WHEN** a recipe display carries an unregistered type id
- **THEN** decoding fails with an invalid-value error
