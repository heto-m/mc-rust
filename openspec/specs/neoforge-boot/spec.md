# neoforge-boot Specification

## Purpose
Boot NeoForge headless with the bridge inside mcr's embedded JVM, chosen from the mod set and refused when mixed with Fabric mods.

## Requirements

### Requirement: NeoForge fetch
`cargo xtask neoforge-fetch` SHALL download the pinned NeoForge installer, its libraries and the vanilla libraries into `.cache/neoforge/` with checksum verification, produce the patched client jar with the installer's own processor, and write a launch manifest. Nothing it downloads SHALL be committed.

#### Scenario: Fresh cache
- **WHEN** `cargo xtask fetch` has run and `cargo xtask neoforge-fetch` runs
- **THEN** `.cache/neoforge/launch.json` names the FML main class, the class path, the library directory, the `--fml.*` arguments and the natives directory, and every listed file exists

### Requirement: Loader chosen from the mod set
mcr SHALL choose the loader from the enabled, resolvable mods: NeoForge when they are NeoForge jars, Fabric when they are Fabric jars. A set holding both SHALL be refused with a message naming a mod of each loader, and no JVM SHALL start.

#### Scenario: NeoForge mods folder
- **WHEN** the mods folder holds only NeoForge jars
- **THEN** Java boots under NeoForge with those jars and the bridge

#### Scenario: Mixed folder
- **WHEN** the enabled mods include a Fabric jar and a NeoForge jar
- **THEN** mcr prints that it cannot mix Fabric and NeoForge mods, names one of each, and runs without Java

### Requirement: NeoForge launch inside mcr
Under NeoForge, mcr SHALL start FML's client entry point in the embedded JVM with FML's production layout, add the bridge and the mods without copying them into Java's `mods/`, keep FML's early window and version check off, and allow native access for the game module layer. The bridge's mixins and FFM table SHALL work as under Fabric.

#### Scenario: Title screen
- **WHEN** NeoForge boots with the bridge and no other mods
- **THEN** the bridge reports its init, `neoforge` and `mcr_bridge` appear in the mod list, the title screen opens, no SDL window or OpenAL device opens, and no TCP socket is open

#### Scenario: Missing NeoForge build of the bridge
- **WHEN** the bridge jar was built without NeoForge's loader jar
- **THEN** the boot fails before the JVM starts with a message naming `cargo xtask neoforge-fetch` and `cargo xtask java-build`

### Requirement: Warm-up stays Fabric-only
The AOT cache and class preloading SHALL be off under NeoForge, and the startup log SHALL say so.

#### Scenario: NeoForge start
- **WHEN** Java starts under NeoForge
- **THEN** no AOT or class-load log option is passed and the log says the cache and preloading are off for NeoForge
