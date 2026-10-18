## ADDED Requirements

### Requirement: NeoForge compatibility record
`docs/mod-bridge.md` SHALL keep a scoreboard of swept NeoForge mods with their version, whether each loaded, joined a vanilla server and showed its main feature, and a one-line reason for anything short of working. NeoForge sweeps SHALL follow the Fabric sweep procedure, and their jars SHALL stay in `.cache/mods-neoforge/`, out of the repository.

#### Scenario: Reading a NeoForge result
- **WHEN** a user looks up a swept NeoForge mod
- **THEN** the scoreboard gives its load, join and feature result and why
