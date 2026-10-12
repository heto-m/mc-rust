## ADDED Requirements

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
