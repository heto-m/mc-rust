# Proposal

## Why

Singleplayer runs natively through a managed vanilla server, but in mod mode Java's real client draws the title
screen and its Singleplayer button only shows a "not supported" toast. Players with mods cannot play local worlds.

## What Changes

- **Java's own world screens, mcr's server.** Java's `SelectWorldScreen` and `CreateWorldScreen` stay (mods that
  add buttons or info to them keep working). Java's world storage points at mcr's `<game dir>/saves`. "Play" and
  "Create" are intercepted before Java opens the world: Rust starts its managed server for that world and joins
  it through the net bridge exactly as for multiplayer. Java's integrated server never starts.
- **Create options mapped.** Name, game mode (survival, hardcore, creative), difficulty, allow commands, world
  type (default, superflat, large biomes, amplified), seed, structures and bonus chest go to mcr's create
  settings. Other world types are refused on Java's screen with a toast; data packs, experiments and game rule
  edits are not applied (a toast says so for packs and experiments).
- **Progress, EULA and failures in Java.** Java shows a progress screen with mcr's status lines, a Java EULA
  prompt the first time, and a failure screen that returns to the world list.
- **No Java writes while the server runs.** While mcr's local world is preparing, running or stopping, Java
  refuses to open any world folder (`LevelStorageSource` access) and never reaches `Minecraft.doWorldLoad`.
- **Pause.** Java publishes vanilla's `Gui.isPausing()`; in a local world mcr freezes the server tick while it
  holds and unfreezes when it stops. Java's own client pauses like vanilla singleplayer.
- **Save and Quit.** Java's pause screen says "Save and Quit to Title"; leaving goes through mcr's save and stop
  path and Java shows "Saving world" until the server exited, then its title screen. Open to LAN never appears
  (Java has no integrated server).
- **Connection loss.** A server crash or kick shows Java's disconnected screen leading back to the world list.
- **ABI 17.** `MSG_WORLD` (Rust to Java), `REPORT_WORLD` (Java to Rust) and the `JAVA_PAUSING` input flag.
- `--singleplayer <folder>` in mod mode waits for Java to finish loading, then uses the same Java flow.

## Out of scope

Server-side mods (the server is vanilla), Open to LAN, game rule, data pack and experiment settings at creation,
customised superflat layers, Realms, writing `icon.png`.

## Capabilities

### New Capabilities
- `mod-singleplayer`: Java's world screens over mcr's saves and managed server, pause and save-and-quit in mod mode.

### Modified Capabilities

## Impact

`java/mcr-bridge` (world mixins, progress and saving screens), `mcr-modbridge` (ABI, events, world messages,
saves launch flags), `mcr-app` (shell singleplayer flow in mod mode, mod host).
