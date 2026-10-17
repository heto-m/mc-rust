# Proposal

## Why

Milestone 1 (full play against a vanilla server) is done, but the title screen's Singleplayer button is still
disabled. The fixed decision for singleplayer is to run Mojang's own server jar as a managed child process on
loopback, so local worlds use the existing connection path and stay in the vanilla format.

## What Changes

- **Runtime provisioning.** The 26.3 server jar is downloaded at runtime from Mojang's version manifest into
  `<game dir>/versions/26.3/`, its SHA-1 checked against the version JSON. Java comes from the same search the
  mod layer uses (`MCR_JAVA_HOME`, the dev JDK, `JAVA_HOME`, `java` on the path), falling back to Mojang's own
  `java-runtime-epsilon` component downloaded into `<game dir>/runtime/`.
- **World list.** Singleplayer opens a world list read from `<game dir>/saves/*/level.dat` (name, folder, last
  played, game mode, cheats, version, `icon.png`), newest first, with Play, Create New World, Edit (rename) and
  Delete (confirmed).
- **Create New World.** Name, game mode (survival, hardcore, creative), difficulty, allow cheats, world type
  (default, superflat, large biomes, amplified), seed, generate structures and bonus chest, mapped to
  `server.properties`, `ops.json` and `--bonusChest`.
- **Managed server.** One server at a time, `127.0.0.1` on a free port, offline mode, its own process group,
  killed if mcr dies (Linux `PR_SET_PDEATHSIG`), memory-capped in a `systemd-run --user --scope` when
  available. mcr waits for `Done` (bounded, showing "Preparing spawn area"), then joins. Console commands go
  through stdin; output goes to `<world>/mcr-server/logs/console.log`.
- **Pause and quit.** The singleplayer pause screen freezes the server tick (`tick freeze` on the console) and
  unfreezes on close. "Save and Quit to Title" disconnects, sends `save-all flush` and `stop`, waits (bounded,
  then kill) and returns to the world list. A server left over from a crash is found through its pid file and
  `session.lock` and stopped before the world opens again.

## Out of scope

Open to LAN, Re-Create, world backups, data packs and experiments screens, the single-biome and customised
superflat presets, writing `icon.png`, the search box of the world list, and singleplayer from Java's title
screen in mod mode (it keeps its "not supported" toast).

## Impact

- New crate `crates/mcr-singleplayer` (layout, level.dat, properties, provisioning, Java runtime, server
  process, stale-server cleanup).
- `crates/mcr-ui`: world list, create and edit world screens, new screen actions, Singleplayer button, pause
  screen pausing in singleplayer.
- `crates/mcr-app`: singleplayer session in the shell, `--singleplayer <folder>`, autopilot `wait_idle`.
