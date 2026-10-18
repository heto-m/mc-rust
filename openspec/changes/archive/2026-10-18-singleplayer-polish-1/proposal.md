# Proposal

## Why

Local worlds run on a managed vanilla server, but Open to LAN, world icons, Re-Create and several creation
options (game rules, data packs, experiments, custom superflat layers) are missing, and mod mode has small gaps
left from `mod-singleplayer`.

## What Changes

- **Open to LAN** from the native pause menu and, in mod mode, from a LAN section in Java's World Options: game
  mode, allow cheats, port. The server stays on loopback; a Rust TCP forwarder (blocking threads, bounded buffers
  and connections) listens on the chosen port, and an announcer multicasts vanilla's
  `[MOTD]…[/MOTD][AD]port[/AD]` every 1.5 s. Game mode and cheats go through console commands
  (`defaultgamemode`, `gamemode`, `op`/`deop` for the host); chat shows vanilla's line; both stop on Save and Quit.
- **World icons.** The native list shows a bounded 64×64 `icon.png`. A local world without one gets a capture of
  mcr's world image (centre square, 64×64) taken when the game pauses, written symlink-safely on Save and Quit.
- **Re-Create** in the native world list opens Create New World filled from the world's saved settings.
- **Creation options applied**, native and Java: game rules (console `gamerule` on the first start), data packs
  (validated copy into `<world>/datapacks`), experiments (`initial-enabled-packs`), custom superflat layers
  (`generator-settings`). Single biome and debug stay refused, with a test.
- **Mod mode gaps:** "Save and Quit to Title" on the custom-dialog warning screen; after a crash, mcr's reason with
  Back to World List; `createFreshLevel` from a mod refused before a folder exists.
- **ABI 18** (or the next free number at merge): `lan` request, `WORLD_LAN` message.

## Out of scope

Online publishing, 26.3's guest command access and force game mode, closing LAN without leaving, server-side
mods, single-biome and debug worlds, backups.

## Capabilities

### New Capabilities
- `singleplayer-lan`: Open to LAN for the managed server (forwarder, announcer, commands, native and Java screens).

### Modified Capabilities
- `singleplayer-server`: creation applies rules, packs, experiments, superflat layers; Save and Quit writes the icon.
- `world-select-screens`: Re-Create; game rules, data packs and superflat screens on Create New World.
- `mod-singleplayer`: richer create requests, LAN in World Options, crash screen, warning label, `createFreshLevel`.

## Impact

`mcr-singleplayer`, `mcr-render` (frame capture), `mcr-ui`, `mcr-app`, `mcr-modbridge`, `java/mcr-bridge`.
