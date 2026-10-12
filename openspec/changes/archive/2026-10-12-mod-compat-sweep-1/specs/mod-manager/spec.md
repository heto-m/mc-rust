## ADDED Requirements

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
