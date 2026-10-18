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
mcr SHALL choose the loader from the enabled, resolvable mods: NeoForge when they are NeoForge jars, Fabric when they are Fabric jars. A set holding both SHALL be refused with a message naming a mod of each loader, and no JVM SHALL start. A multi-loader jar (both `fabric.mod.json` and `META-INF/neoforge.mods.toml`) SHALL NOT decide the loader: it SHALL be evaluated as a NeoForge mod when the other mods choose NeoForge, and as a Fabric mod otherwise.

#### Scenario: NeoForge mods folder
- **WHEN** the mods folder holds only NeoForge jars
- **THEN** Java boots under NeoForge with those jars and the bridge

#### Scenario: Mixed folder
- **WHEN** the enabled mods include a Fabric jar and a NeoForge jar
- **THEN** mcr prints that it cannot mix Fabric and NeoForge mods, names one of each, and runs without Java

#### Scenario: Multi-loader jar with NeoForge mods
- **WHEN** the enabled mods are a multi-loader jar and NeoForge jars
- **THEN** Java boots under NeoForge and the multi-loader jar is active with its NeoForge metadata

#### Scenario: Multi-loader jar alone
- **WHEN** the only enabled mod is a multi-loader jar
- **THEN** it is evaluated as a Fabric mod

### Requirement: NeoForge launch inside mcr
Under NeoForge, mcr SHALL start FML's client entry point in the embedded JVM with FML's production layout, add the bridge and the mods without copying them into Java's `mods/`, keep FML's early window and version check off, and allow native access for the game module layer. The bridge's mixins and FFM table SHALL work as under Fabric.

#### Scenario: Title screen
- **WHEN** NeoForge boots with the bridge and no other mods
- **THEN** the bridge reports its init, `neoforge` and `mcr_bridge` appear in the mod list, the title screen opens, no SDL window or OpenAL device opens, and no TCP socket is open

#### Scenario: Missing NeoForge build of the bridge
- **WHEN** the bridge jar was built without NeoForge's loader jar
- **THEN** the boot fails before the JVM starts with a message naming `cargo xtask neoforge-fetch` and `cargo xtask java-build`

### Requirement: Warm-up under NeoForge
NeoForge sessions SHALL record, build and use the per-mod-set AOT cache and record and preload the class list as Fabric sessions do, with the same bounds, strikes and `MCR_JVM_AOT`/`MCR_JVM_PRELOAD` switches.

#### Scenario: Second NeoForge session
- **WHEN** a NeoForge mod set was launched once, quit cleanly and its cache was built
- **THEN** the next launch starts from the cache, preloads its class list, and the cache is not refused

### Requirement: NeoForge load failure without dialogs
Under NeoForge a mod failing while mods load SHALL end the VM with FML's exit code and a crash report, without any native dialog or message box, and mcr SHALL go on natively.

#### Scenario: Mod fails at load
- **WHEN** a mod throws in its constructor under NeoForge
- **THEN** the VM exits with code 1 after writing one crash report, no dialog or message box opens, and mcr keeps running

### Requirement: NeoForge crash and exit
Under NeoForge a crash in game SHALL write a crash report and detach the bridge, a render thread that stops answering SHALL detach the bridge within 3 s while mcr keeps rendering, `System.exit` from a mod SHALL end the VM with its code, and a clean quit SHALL destroy the VM. The bridge SHALL offer opt-in test hooks for the in-game crash and the hang under both loaders.

#### Scenario: Crash in game
- **WHEN** the in-game crash hook fires under NeoForge
- **THEN** the bridge reports a crash with a report file, and shutdown asks for a process exit

#### Scenario: Hang
- **WHEN** the hang hook blocks Java's render thread under NeoForge
- **THEN** the bridge detaches within 3 s and mcr's frames go on

#### Scenario: Clean quit
- **WHEN** a NeoForge session at the title screen shuts down
- **THEN** the VM is destroyed and the shutdown is clean

### Requirement: NeoForge client commands
While bridged, a command typed in Java's chat under NeoForge SHALL first go to NeoForge's client command dispatcher, and only a command it does not handle SHALL go to Rust for the server.

#### Scenario: Client-side command
- **WHEN** spark is loaded under NeoForge and the player types `/sparkc tps` in Java's chat
- **THEN** spark answers in chat and nothing is sent to the server

#### Scenario: Server command
- **WHEN** the player types a server command in Java's chat under NeoForge
- **THEN** it reaches the server through Rust as before
