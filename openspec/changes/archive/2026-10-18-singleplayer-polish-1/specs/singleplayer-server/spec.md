## MODIFIED Requirements

### Requirement: Local server configuration
Each launch SHALL write `<world>/mcr-server/server.properties` keeping unknown user keys and forcing `server-ip=127.0.0.1`, a free loopback port, `online-mode=false`, `enforce-secure-profile=false`, `max-players=8` (vanilla's integrated server), `spawn-protection=0`, view and simulation distance from the client options, and console broadcasts off. Game mode, difficulty and hardcore SHALL come from the creation settings or from `level.dat`.

#### Scenario: Reopened world
- **WHEN** a world whose `level.dat` says creative on hard is opened
- **THEN** the properties say `gamemode=creative` and `difficulty=hard` with the forced network keys

### Requirement: Creation settings
A new world SHALL map its seed, world type, generate-structures flag, game mode, difficulty and hardcore flag to `server.properties`, and its bonus chest to `--bonusChest` on the first launch only. On that first launch it SHALL also write chosen experiments as `initial-enabled-packs=vanilla,<packs>` (known feature packs only), custom superflat layers as validated `generator-settings` JSON (at most 64 layers, total height at most 4064, namespaced ids), copy chosen data packs into `<world>/datapacks` (a `.zip` up to 64 MiB starting with a zip header, or a real directory with `pack.mcmeta` within 10,000 entries and 256 MiB, never through symlinks, at most 64 packs), and send `gamerule <id> <value>` for each changed game rule (known 26.3 rules, values within their bounds) once the server is ready and before joining. With cheats allowed, `ops.json` SHALL make the player (offline UUID, as the client computes it) operator level 4; otherwise it SHALL be empty. The choice SHALL be remembered for later launches.

#### Scenario: Cheats on
- **WHEN** a world is created with Allow Cheats on for player `Steve`
- **THEN** `ops.json` holds Steve's offline UUID with level 4

#### Scenario: Keep inventory and a data pack
- **WHEN** a world is created with keep inventory on and the data pack `tools.zip` selected
- **THEN** `<world>/datapacks/tools.zip` exists before the server starts and the console receives `gamerule keep_inventory true` before the player joins

#### Scenario: Custom superflat layers
- **WHEN** a superflat world is created with the preset `minecraft:bedrock,3*minecraft:stone;minecraft:desert`
- **THEN** `generator-settings` holds those layers and biome as JSON

#### Scenario: Symlinked data pack
- **WHEN** a selected data pack directory contains a symbolic link
- **THEN** the world is not created and the failure names the pack

## ADDED Requirements

### Requirement: World icon on save
When a local world has no `icon.png`, mcr SHALL capture its own world image (without GUI) when the game starts pausing, crop it to the centre square, scale it to 64×64, and on Save and Quit write it as `<world>/icon.png` through the symlink-safe writer, only if the file is still missing.

#### Scenario: First save
- **WHEN** a new world is played, paused and saved and quit
- **THEN** its folder holds a 64×64 `icon.png` and the world list shows it

#### Scenario: Planted icon link
- **WHEN** `<world>/icon.png` is a symbolic link
- **THEN** nothing is written through it
