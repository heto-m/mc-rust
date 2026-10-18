# singleplayer-server Specification

## Purpose
The vanilla server as a managed loopback child process: its configuration, memory cap, lifecycle, pause, save and quit, and cleanup of leftover servers.

## Requirements

### Requirement: Local server configuration
Each launch SHALL write `<world>/mcr-server/server.properties` keeping unknown user keys and forcing `server-ip=127.0.0.1`, a free loopback port, `online-mode=false`, `enforce-secure-profile=false`, `max-players=8` (vanilla's integrated server), `spawn-protection=0`, view and simulation distance from the client options, and console broadcasts off. Game mode, difficulty and hardcore SHALL come from the creation settings or from `level.dat`.

#### Scenario: Reopened world
- **WHEN** a world whose `level.dat` says creative on hard is opened
- **THEN** the properties say `gamemode=creative` and `difficulty=hard` with the forced network keys

### Requirement: Creation settings
A new world SHALL map its seed, world type, generate-structures flag, game mode, difficulty and hardcore flag to `server.properties`, and its bonus chest to `--bonusChest` on the first launch only. With cheats allowed, `ops.json` SHALL make the player (offline UUID, as the client computes it) operator level 4; otherwise it SHALL be empty. The choice SHALL be remembered for later launches.

#### Scenario: Cheats on
- **WHEN** a world is created with Allow Cheats on for player `Steve`
- **THEN** `ops.json` holds Steve's offline UUID with level 4

### Requirement: Creation experiments and superflat layers
On the first launch a new world SHALL write chosen experiments as `initial-enabled-packs=vanilla,<packs>` (known feature packs only) and custom superflat layers as validated `generator-settings` JSON (at most 64 layers, total height at most 4064, namespaced ids).

#### Scenario: Custom superflat layers
- **WHEN** a superflat world is created with the preset `minecraft:bedrock,3*minecraft:stone;minecraft:desert`
- **THEN** `generator-settings` holds those layers and biome as JSON

### Requirement: Creation data packs and game rules
A new world SHALL copy chosen data packs into `<world>/datapacks` before its first launch (a `.zip` up to 64 MiB starting with a zip header, or a real directory with `pack.mcmeta` within 10,000 entries and 256 MiB, never through symlinks, at most 64 packs), and SHALL send `gamerule <id> <value>` for each changed game rule (known 26.3 rules, values within their bounds) once the server is ready and before joining.

#### Scenario: Keep inventory and a data pack
- **WHEN** a world is created with keep inventory on and the data pack `tools.zip` selected
- **THEN** `<world>/datapacks/tools.zip` exists before the server starts and the console receives `gamerule keep_inventory true` before the player joins

### Requirement: Managed server process
At most one server SHALL run. It SHALL start in its own process group with stdin, stdout and stderr piped, `nogui`, its cwd in `<world>/mcr-server`, and output lines (bounded in length and total size) copied to `logs/console.log`. The client SHALL join once a `Done` line appears and SHALL kill the server if it is not ready within a bounded time, reporting progress from "Preparing spawn area" lines.

#### Scenario: Ready
- **WHEN** the server prints its `Done (…)! For help` line
- **THEN** the client connects to `127.0.0.1` on the chosen port through the normal connection path

#### Scenario: Never ready
- **WHEN** no `Done` line appears before the timeout
- **THEN** the process group is killed and a failure screen is shown

### Requirement: Memory cap
Where `systemd-run --user --scope` works, the server SHALL run inside a scope with `MemoryMax` and `MemorySwapMax=0`; elsewhere it SHALL be a plain child. The heap SHALL be a quarter of system memory clamped to 2–4 GiB, `MCR_SP_HEAP_MIB` overriding it, and the scope limit SHALL be the heap plus 1 GiB.

#### Scenario: 16 GiB machine
- **WHEN** the machine has 16 GiB of memory
- **THEN** the server runs with `-Xmx4096M` in a scope limited to 5120M

### Requirement: Pause freezes the server
In singleplayer, while a pause screen is open, the client SHALL have sent `tick freeze` to the server console, and `tick unfreeze` once no pause screen is open or before quitting.

#### Scenario: Pause and resume
- **WHEN** the player opens the pause menu and then returns to the game
- **THEN** the console receives `tick freeze` and then `tick unfreeze`

### Requirement: Save and quit
Leaving a singleplayer world SHALL disconnect, send `save-all flush` and `stop` on the console, show a saving screen, wait a bounded time for the process to exit and then kill its process group, and return to the world list. Closing the client SHALL do the same before exit.

#### Scenario: Server ignores stop
- **WHEN** the process does not exit within the grace period after `stop`
- **THEN** its process group is killed and the world list is shown

### Requirement: No orphaned servers
On Linux the server SHALL receive SIGTERM when the client process dies. Each start SHALL write a pid file with a per-launch token. Before a world starts, a live process from its pid file or holding its `session.lock` SHALL be stopped (SIGTERM, then SIGKILL) only when proven to be mcr's server for this world (same user, server jar, world marker, token); an unproven lock holder SHALL refuse the start, and nothing else is signalled.

#### Scenario: Client crashed
- **WHEN** the client is killed while a world runs and the world is opened again
- **THEN** no earlier server process is left running and the world starts normally

#### Scenario: Planted pid file
- **WHEN** a downloaded world's pid file names an unrelated process of the same user
- **THEN** that process is not signalled and the world starts

### Requirement: No writes through symlinks
World folders SHALL be treated as untrusted. A world SHALL be a real directory directly inside the canonical saves folder. A launch SHALL refuse a world whose folder, `level.dat`, `level.dat_old`, `session.lock` or anything under `mcr-server` is a symlink. mcr SHALL write world files only through a new temp file (no-follow) renamed over a non-link target, and read them with no-follow opens.

#### Scenario: Planted ops.json link
- **WHEN** a downloaded world ships `mcr-server/ops.json` as a symlink to a file elsewhere
- **THEN** the world does not start, a failure screen names the link, and the other file is unchanged

#### Scenario: Delete with an inner link
- **WHEN** a world containing a symlink to an outside directory is deleted
- **THEN** only the link is removed and the outside directory keeps its files

### Requirement: World icon on save
When a local world has no `icon.png`, mcr SHALL capture its own world image (without GUI) when the game starts pausing, crop it to the centre square, scale it to 64×64, and on Save and Quit write it as `<world>/icon.png` through the symlink-safe writer, only if the file is still missing.

#### Scenario: First save
- **WHEN** a new world is played, paused and saved and quit
- **THEN** its folder holds a 64×64 `icon.png` and the world list shows it

#### Scenario: Planted icon link
- **WHEN** `<world>/icon.png` is a symbolic link
- **THEN** nothing is written through it
