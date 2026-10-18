# Proposal

## Why

`mod-neoforge-1` boots NeoForge and joins servers, but left the rest of mod mode Fabric-only or untried: the
AOT cache and class preloading are off under NeoForge, its mods' config screens, Java's singleplayer flow and the
crash and exit paths were never exercised, the pinned NeoForge version is written down twice, and only fourteen
NeoForge mods were tried.

## What Changes

- **Warm-up under NeoForge.** The per-mod-set AOT cache and background class preloading run under FML's module
  layer. The AOT key also covers the patched game jar and the NeoForge version, which FML reads off the class path.
  The preloader works with FML's `TransformingClassLoader` (already-loaded probe, class processor packages left to
  the game, like Mixin's extensions). The 2-strike safety and the Mixin exclusions stay. Measured before and after.
- **Config screens.** The config screens of JEI, Jade, AppleSkin and BetterF3, opened from NeoForge's mod list,
  open, change and save a value, and reopen with it; what breaks is fixed.
- **Singleplayer under NeoForge.** Java's create, play, pause, Save and Quit and Open to LAN run on mcr's managed
  vanilla server when NeoForge is the loader, as they do under Fabric.
- **Crash and exit tests.** Opt-in JVM tests for NeoForge: a mod failing at load, a crash in game, a hang that
  detaches within 2 s, `System.exit`, and a clean quit. The bridge gains loader-neutral test hooks for the in-game
  crash and the hang.
- **One pin.** `NEOFORGE_VERSION` (and Fabric's `LOADER_VERSION`) live in one file shared by `mcr-modbridge` and
  `xtask`.
- **Compat sweep.** Fifteen more popular client-side NeoForge mods for 26.3 from Modrinth, each checked for load,
  join and main feature; the scoreboard goes into `docs/mod-bridge.md`.

## Out of scope

A NeoForge server for singleplayer, Forge, Quilt, NeoForge versions other than the pinned one, and fixing mods that
need OpenGL.

## Impact

- `crates/mcr-modbridge`: warm-up for both loaders, AOT key inputs, the shared pin file, NeoForge JVM tests.
- `crates/mcr-app`: the "Fabric only" warm-up notice goes.
- `java/mcr-bridge`: loader-neutral preloader (`GameClasses`), test hooks, fixes found by the config, singleplayer
  and sweep work.
- `xtask`: reads the pins from the shared file.
- No ABI change: `BRIDGE_VERSION` stays 19.
- Specs: `neoforge-boot` (warm-up, crash and exit), `jvm-warmup` (FML class processors, key inputs),
  `mod-singleplayer` (NeoForge), `mod-compat` (NeoForge sweep). Docs: `mod-bridge.md`, `mod-compat.md`.
