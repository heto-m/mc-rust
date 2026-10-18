# mod-registries Specification

## Purpose
Modded registries synced by a modded server reach Rust's tables, so terrain, physics, light and baking follow the server's numbering.

## Requirements

### Requirement: Registries from Java after configuration
In mod mode, Java SHALL report its registries once at the title screen and again at the end of each configuration phase, after Fabric's registry sync: every static registry's names from id 0, and every block, block state, item, entity type and fluid with the data the vanilla extractor writes. The report SHALL be at most 64 MiB.

#### Scenario: Modded server
- **WHEN** mcr in mod mode joins a Fabric server running Echoes of the Sift's mods
- **THEN** the log reports 70472 block states from Java

### Requirement: Tables rebuilt before play
Rust SHALL rebuild its tables from Java's report, make them active, and only then acknowledge the end of configuration, waiting at most 10 seconds. A report Rust refuses (vanilla prefix reordered, inconsistent counts, invalid values, more than 2^20 block states) SHALL keep the previous tables and be logged.

#### Scenario: Modded chunks
- **WHEN** the first chunk with modded blocks arrives
- **THEN** it decodes with the rebuilt tables and renders

#### Scenario: A property added to vanilla blocks
- **WHEN** a mod adds a block state property to vanilla waterloggable blocks
- **THEN** the state ids after the first changed block move as Java numbers them, and vanilla's blocks keep their shapes and light

### Requirement: Block states beyond 16 bits
Block state ids SHALL be 32-bit across world storage, meshing, physics, lighting and animation, bounded by the active table's count (at most 2^20). A paletted section update with a state past the active count SHALL be dropped.

#### Scenario: Hostile section update
- **WHEN** a section update carries a state id beyond the registry's state count
- **THEN** that update is ignored and the section is unchanged

### Requirement: Modded blocks bake from mod jars
Mod jars SHALL join Rust's resource stack as assets-only packs (with nested jars that contain assets, bounded in count and size), and the model cache SHALL be rebaked when the active tables change. States whose models cannot be baked SHALL be counted and logged by name. Biome colours SHALL come from the server's registry data, with the client jar as fallback. An item model definition that cannot be parsed SHALL skip only that item.

#### Scenario: Modded block in view
- **WHEN** a modded block with a JSON block model is placed near the player
- **THEN** mcr's terrain shows it with its mod textures
