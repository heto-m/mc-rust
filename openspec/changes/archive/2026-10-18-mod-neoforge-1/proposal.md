# Proposal

## Why

Mod mode runs only Fabric, and NeoForge jars are listed as "not supported yet". The mod scan chose "Fabric first,
NeoForge second (same seams)". NeoForge 26.3 (26.3.0.90) runs FancyModLoader 12, which dropped ModLauncher and
SecureJarHandler, uses the same `sponge-mixin` build as our Fabric Loader and a plain module layer, so the bridge
can run under it nearly unchanged.

## What Changes

- **Fetch.** `cargo xtask neoforge-fetch` downloads the pinned installer from maven.neoforged.net into
  `.cache/neoforge/`, fetches its and the vanilla libraries by checksum, runs the installer's own client processor
  to patch the client jar, and writes `.cache/neoforge/launch.json`.
- **One bridge jar.** `mcr-bridge.jar` carries `fabric.mod.json` and `META-INF/neoforge.mods.toml`, one mixin
  config, and an entry class per loader that fills a loader-neutral `Loaders` seam. Mixins NeoForge's patches
  bypass get descriptor-exact targets.
- **Loader choice.** `Loader` (`Fabric`, `NeoForge`) follows the enabled mods. A set that needs both is refused:
  each mod shows the reason on the Mods screen, mcr prints it once, and no JVM starts.
- **NeoForge launch.** FML's `Client` runs through `McrBoot launch` with FML's production layout; mods come from
  `fml.modFolders`, FML's early window and version check are off, and the game layer gets native access through
  `--illegal-native-access=allow` (Fabric's `ALL-UNNAMED` counterpart).
- **Warm-up.** AOT cache and class preloading stay Fabric-only.
- **Mods screen.** NeoForge jars and `FMLModType` libraries are read (id, version, name, description, icon,
  required and incompatible dependencies, jar-in-jar) and go through the dependency pre-check.
- **Tests.** Opt-in JVM test `neoforge_boot`; unit tests for loader detection, mixed sets, the launch plan, the
  fetch profile parsing and the TOML reader. `fabric_boot`'s no-socket check, broken since Mojang's discovery host
  was allow-listed, runs with an empty proxy allow-list.

## Out of scope

AOT cache and preloading under NeoForge, NeoForge config screens, singleplayer world work under NeoForge (only
checked not to crash), Forge, Quilt, and a NeoForge server for singleplayer.

## Impact

- `xtask`: `neoforge-fetch`; `java-build` gains an optional `neoforge-launch` dependency.
- `crates/mcr-modbridge`: `Loader`, NeoForge manifest and plan, warm-up gated on Fabric, scan and resolver.
- `crates/mcr-app`: loader choice and refusal message.
- `java/mcr-bridge`: loader entry layer, `neoforge.mods.toml`, `RenderSectionMixin` overloads.
- No ABI change: `BRIDGE_VERSION` stays 19.
- Specs: new `neoforge-boot`; `mod-manager` scanning changes. Docs: `mod-bridge.md`, `mod-compat.md`.
