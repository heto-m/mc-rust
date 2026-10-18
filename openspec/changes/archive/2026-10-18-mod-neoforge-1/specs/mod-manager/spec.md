## MODIFIED Requirements

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
