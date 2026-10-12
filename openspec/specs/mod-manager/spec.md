# mod-manager Specification

## Purpose
Scan the mods folder, apply the compatibility policy and dependency pre-check, keep enable state, and offer a native Mods screen and crash fallback.

## Requirements

### Requirement: Mods folder scanning
mcr SHALL scan `<data>/mods/` for jars, read Fabric metadata (id, name, version, description, icon, dependencies, breaks, environment, provides, nested jars) without loading classes, and SHALL list NeoForge/Forge jars and unparsable jars with a reason instead of loading them.

#### Scenario: NeoForge jar
- **WHEN** a NeoForge mod jar is in the mods folder
- **THEN** it is listed as not supported yet and is not passed to the loader

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
