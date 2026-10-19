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
Mod jars SHALL join Rust's resource stack as assets-only packs (with nested jars that contain assets, bounded in count and size), and the model cache SHALL be rebaked when the active tables change. States whose models Rust cannot bake SHALL be meshed from Java's baked quads when Java reported them, and otherwise counted and logged by name.

#### Scenario: Modded block in view
- **WHEN** a modded block with a JSON block model is placed near the player
- **THEN** mcr's terrain shows it with its mod textures

#### Scenario: Code-generated model
- **WHEN** Easy Anvils' anvil, whose model has no block model JSON Rust can read, is placed near the player
- **THEN** mcr's terrain shows it with the quads Java baked

### Requirement: Modded biome colours and item models
Biome colours SHALL come from the server's registry data, with the client jar as fallback. An item model definition that cannot be parsed SHALL skip only that item.

#### Scenario: Broken item model definition
- **WHEN** one mod's item model definition cannot be parsed
- **THEN** only that item lacks a model and every other item and entity model still loads

### Requirement: Java's models on request
Java SHALL report its fluid models at the title screen, and on request the baked quads of named blocks for every state, with each state's ambient occlusion, particle sprite and tint sources. States with an invisible render shape SHALL have no quads. Rust SHALL request only blocks it could not bake, each once per session.

#### Scenario: Unbaked block at the title
- **WHEN** the title prebake finds blocks without a readable model
- **THEN** Rust asks Java for those blocks and the next bake uses Java's quads

#### Scenario: Liquid block
- **WHEN** Rust asks for a modded liquid block without a block model
- **THEN** Java reports its states without quads and only the fluid renders

### Requirement: Render report bounds
Rust SHALL refuse a render report past 64 MiB, 4096 blocks, 4096 states per block, 4096 quads per state, 2^20 quads in total or 65536 sprites, and keep the models it had.

#### Scenario: Oversized report
- **WHEN** a report lists more than 4096 quads for one state
- **THEN** the report is refused and logged, and the terrain keeps its previous models

### Requirement: Modded fluids
Rust SHALL mesh each modded fluid with its own still, flowing and overlay sprites, render layer and tint from Java's fluid model, as a fluid family of its own (at most 255).

#### Scenario: Ichor pool
- **WHEN** a pool of Echoes of the Sift's ichor is in view
- **THEN** it renders as a fluid surface with the ichor's animated texture, not as a missing-texture block

### Requirement: Modded wire shapes
Java SHALL describe the stream codecs of modded data component types, particle types and entity data serializers built from vanilla's codec factories as wire shapes in its registry report. Rust SHALL decode values of those types by shape, keeping data component and particle bytes, within bounds (shapes of at most 1024 tokens and depth 24, at most 65536 steps and 2 MiB per value). A modded type without a shape SHALL leave its packet undecodable, dropped and counted.

#### Scenario: Modded component on an item
- **WHEN** the server sends equipment whose item stack carries a modded component type with a shape
- **THEN** Rust decodes the packet instead of dropping it

#### Scenario: Hostile value
- **WHEN** a modded value claims a length past the packet or nests past the bounds
- **THEN** the packet fails to decode without panicking and is dropped

### Requirement: Title prebake
In mod mode, Rust SHALL bake the model cache for Java's title registries in the background at the title screen, and at the join SHALL use it re-indexed for the server's tables when the same blocks with the same states and render inputs are only renumbered, waiting at most 10 seconds for a running prebake; otherwise it SHALL bake at the join. When Java answers a model request after the join, Rust SHALL rebake in the background and swap the cache in.

#### Scenario: Modded join
- **WHEN** mcr joins a Fabric server running Medieval Immersed's mods after the prebake finished
- **THEN** the log reports models prebaked, and no bake runs on the join

### Requirement: Java's resource packs in the terrain
Rust's resource stack SHALL include the resource packs Java has selected that lie inside the game dir, between mod jars and server packs, and SHALL rebake when the selection changes.

#### Scenario: Texture pack in the pack
- **WHEN** the pack enables a 32x texture pack in Java
- **THEN** mcr's terrain uses its block textures

### Requirement: Java's packs predicted at startup
In mod mode, mcr SHALL build its first resource stack with the resource packs that Java's `options.txt` selects, before Java reports its selection. It SHALL read at most 256 `file/` entries, SHALL reject names that contain path separators or are `.` or `..`, and SHALL keep only packs that resolve inside the game dir. mcr SHALL compare its stack with Java's selection only after Java's first render report, and SHALL rebake only when they differ.

#### Scenario: Selection unchanged
- **WHEN** Java's options select 16 packs and Java reports the same 16
- **THEN** mcr loads its resources once at startup and does not reload them at the title screen

#### Scenario: Hostile options
- **WHEN** Java's options list `file/../../etc` or a symlink that leaves the game dir
- **THEN** that entry is ignored
