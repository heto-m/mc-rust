# mod-manager Specification

## Purpose
Scan the mods folder, apply the compatibility policy and dependency pre-check, keep enable state, and offer a native Mods screen and crash fallback.

## Requirements

### Requirement: Mods folder scanning
mcr SHALL scan `<data>/mods/` for jars and read Fabric metadata (id, name, version, description, icon, dependencies, breaks, environment, provides, nested jars) and NeoForge metadata from `META-INF/neoforge.mods.toml` (id, name, version, description, required dependencies and their sides) without loading classes. It SHALL list Forge, Quilt and unparsable jars with a reason instead of loading them.

#### Scenario: NeoForge jar
- **WHEN** a NeoForge mod jar is in the mods folder
- **THEN** it is listed with its mod id, name and version and is passed to the loader when enabled

#### Scenario: NeoForge dependency
- **WHEN** an enabled NeoForge mod requires a mod id that no enabled jar provides
- **THEN** it is excluded and the Mods screen shows the missing id

#### Scenario: NeoForge library jar
- **WHEN** a jar has no `neoforge.mods.toml` but its manifest declares `FMLModType: GAMELIBRARY`
- **THEN** it is listed as a NeoForge jar and passed to the loader when enabled

#### Scenario: Forge jar
- **WHEN** a Forge mod jar is in the mods folder
- **THEN** it is listed as not supported and is not passed to the loader

### Requirement: Compatibility policy
mcr SHALL apply a data-driven policy of `blocked`, `unsupported` and `partial` mod ids, each with a reason. Blocked mods SHALL never be passed to the loader. Unsupported mods SHALL require explicit enabling with a warning.

#### Scenario: Sodium present
- **WHEN** Sodium is in the mods folder
- **THEN** the Mods screen shows it as blocked with its reason and the loader does not receive it

### Requirement: Dependency pre-check
Before booting Java, mcr SHALL resolve required dependencies, version ranges and breaks against the enabled set, the game version 26.3, the pinned loader version and Java 25. Mods that cannot be resolved SHALL be excluded for that start with the reason shown.

#### Scenario: Missing dependency
- **WHEN** an enabled mod requires a mod id that is absent
- **THEN** it is excluded and the Mods screen shows the missing id

### Requirement: Enable state and restart
Enabled state SHALL persist in mcr settings by mod id. Changes SHALL take effect on the next start, and the Mods screen SHALL show that a restart is required. With no enabled, resolvable mods, mcr SHALL not start a JVM.

#### Scenario: All disabled
- **WHEN** every mod is disabled and mcr starts
- **THEN** no `mcr-java` thread exists and the client runs natively

### Requirement: Crash fallback
After a session in which Java crashed or hung, the next start SHALL offer to start without mods and SHALL show the crash report path.

#### Scenario: Crash at boot
- **WHEN** a mod crashed Java during the previous boot
- **THEN** mcr shows the fallback prompt before starting Java again

### Requirement: Crash prompt retry
"Try again" on the crash prompt SHALL start a new mcr process with the same arguments and then exit this one, before any JVM is started.

#### Scenario: Retry
- **WHEN** the user chooses "Try again"
- **THEN** a new mcr starts with the same command line and the old one quits

### Requirement: Nested jar depth
The scan SHALL read nested jar manifests two levels deep within a tree-wide byte and jar budget, so ids provided at depth two satisfy dependencies.

#### Scenario: Dependency at depth two
- **WHEN** a mod's nested jar bundles another jar providing a required id
- **THEN** the dependency resolves

### Requirement: Loader-provided mods
The dependency pre-check SHALL treat `mixinextras`, bundled with the pinned Fabric Loader, as present at the bundled version.

#### Scenario: Mod requiring MixinExtras
- **WHEN** an enabled mod requires `mixinextras` within the bundled version
- **THEN** it is not excluded

### Requirement: Lenient manifests
The scan SHALL accept `fabric.mod.json` files that Fabric Loader accepts despite raw control characters inside strings.

#### Scenario: Newline in a description
- **WHEN** a manifest's description string contains a raw newline
- **THEN** the mod is read with its id and the newline shown as a space

### Requirement: Startup report
At startup, mcr SHALL print one line per mod that is left out or loaded with a policy caveat, naming the mod, its version and the reason.

#### Scenario: Blocked mod
- **WHEN** a blocked mod is in the mods folder
- **THEN** the log names it with the policy reason

### Requirement: Automatic heap
Without a chosen heap, the JVM heap SHALL follow the enabled client mod count: 2.3 times an estimated live heap of 510 MiB plus 4.8 MiB a mod, in 256 MiB steps, at least 2048 MiB, and at most 12288 MiB, three quarters of system memory and system memory less 4608 MiB.

#### Scenario: Small set
- **WHEN** 12 client mods are enabled and no heap was chosen
- **THEN** Java starts with 2048 MiB

#### Scenario: Large set
- **WHEN** 102 client mods are enabled and no heap was chosen
- **THEN** Java starts with 2304 MiB

### Requirement: Chosen heap
A heap chosen on the command line, in the launcher profile or in the Mods screen SHALL win over the automatic one, capped at 12288 MiB and three quarters of system memory. A settings file holding 2048 without `heap_explicit` SHALL read as automatic.

#### Scenario: Old settings file
- **WHEN** `mcr-mods.json` holds `"jvm": {"heap_mib": 2048}` and 102 client mods are enabled
- **THEN** Java starts with the automatic 2304 MiB

### Requirement: Heap shown
The heap Java starts with and how it was chosen SHALL be logged at boot and shown in the Mods screen, marked "(auto)" when automatic.

#### Scenario: Mods screen
- **WHEN** the Mods screen opens with 102 client mods and no chosen heap
- **THEN** its heap slider reads "JVM heap: 2304 MiB (auto)"
