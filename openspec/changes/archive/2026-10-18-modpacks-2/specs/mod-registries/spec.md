## MODIFIED Requirements

### Requirement: Modded blocks bake from mod jars
Mod jars SHALL join Rust's resource stack as assets-only packs (with nested jars that contain assets, bounded in count and size), and the model cache SHALL be rebaked when the active tables change. States whose models Rust cannot bake SHALL be meshed from Java's baked quads when Java reported them, and otherwise counted and logged by name. Biome colours SHALL come from the server's registry data, with the client jar as fallback. An item model definition that cannot be parsed SHALL skip only that item.

#### Scenario: Modded block in view
- **WHEN** a modded block with a JSON block model is placed near the player
- **THEN** mcr's terrain shows it with its mod textures

#### Scenario: Code-generated model
- **WHEN** Easy Anvils' anvil, whose model has no block model JSON Rust can read, is placed near the player
- **THEN** mcr's terrain shows it with the quads Java baked

## ADDED Requirements

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
