## REMOVED Requirements

### Requirement: Warm-up stays Fabric-only
**Reason**: The AOT cache and class preloading now work under NeoForge.
**Migration**: None; NeoForge sessions record and use the cache and class list like Fabric sessions.

## MODIFIED Requirements

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

## ADDED Requirements

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
