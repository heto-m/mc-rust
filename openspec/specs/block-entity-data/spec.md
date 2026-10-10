# block-entity-data Specification

## Purpose
Parse block entity NBT from chunk data and update packets into typed, bounded per-kind state.

## Requirements

### Requirement: Kind resolution
The system SHALL map block entity type registry names to a `BlockEntityKind` and report kinds without a renderer as unrendered.

#### Scenario: Chest family
- **WHEN** the type names `minecraft:chest`, `minecraft:trapped_chest` and `minecraft:ender_chest` are resolved
- **THEN** each resolves to its chest kind

### Requirement: Typed parsing with limits
The system SHALL parse sign text, banner patterns, skull profiles, decorated pot sherds, spawner entity ids, vault display items, campfire items, brushable items and shelf items from an NBT compound, treating missing or malformed fields as defaults, and SHALL bound every string, list and text component derived from the tag.

#### Scenario: Oversized banner patterns
- **WHEN** a banner tag lists more than 16 patterns
- **THEN** only the first 16 are kept

#### Scenario: Hostile sign text
- **WHEN** a sign message is a component nested far beyond the allowed depth or longer than the limit
- **THEN** it is truncated or replaced by empty text without panic

#### Scenario: Skull profile
- **WHEN** a player head carries a profile name longer than 16 characters
- **THEN** the name is cut to 16 characters
