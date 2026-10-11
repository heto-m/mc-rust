# Proposal

## Why

With `mcr-jvm` in place, the next proof is that the stock 26.3 `client.jar` boots under Fabric Loader inside
`mcr` with real mods. It must do so headless: vanilla must not open a window, initialise a GPU, open an audio
device or create sockets, because Rust owns all of those. This fixes the boot path and the bridge mod skeleton that
every later seam extends.

## What Changes

- `xtask fabric-fetch`:
  - Reads the Fabric meta launcher profile for 26.3 and the pinned loader version (0.19.5; Fabric API needs
    0.19.3 or newer).
  - Downloads the loader and its libraries (sponge-mixin, mixinextras, ASM) into `.cache/fabric/`, checksum
    verified.
  - Downloads the LWJGL natives listed in `version.json`.
  - Downloads pinned test mods: Fabric API 0.162.0+26.3 and Mod Menu 21.0.0.
- New crate `mcr-modbridge` (Minecraft-specific). It builds the classpath, VM options and game arguments, starts
  `mcr-jvm`, and runs a frame pacer.
- Java module `java/mcr-bridge/`, a Fabric mod (`mcr_bridge`, our code) with mixins that:
  - Pick an in-mod null GPU backend instead of OpenGL/Vulkan (`PreferredGraphicsApi.getBackendsToTry`).
  - Force SDL3's offscreen video driver (`RenderSystem.initBackendSystem`).
  - Skip OpenAL (`SoundEngine.loadLibrary`).
  - Route `ConnectScreen.connect` to a Rust stub.
  - Pace `Minecraft.runTick` on a Rust doorbell and disable `FramerateLimiter`.
  - Disable `ClientShutdownWatchdog`.
  - Report lifecycle (mod init, screen changes, crash) to Rust.
- `mcr --mods <dir>` runs the boot headless next to the normal client (no visible effect yet beyond logs).

Out of scope: drawing anything, packets, input, sound output, the mod manager UI, NeoForge, macOS/Windows
verification, Realms/telemetry beyond what `--offlineDeveloperMode` disables.

Acceptance test: with Fabric API and Mod Menu, the log shows both mods loaded and their client initializers run.
Java reaches `TitleScreen` (reported through the bridge). No OS window, OpenAL device or socket is created. The JVM
shuts down cleanly on request in under 10 s.

## Capabilities

### New Capabilities
- `fabric-boot`: headless Fabric boot inside `mcr`, the bridge mod skeleton, frame pacing and lifecycle reporting.

### Modified Capabilities
- `dev-tooling`: adds `fabric-fetch`.

## Impact

- New crate `crates/mcr-modbridge` (depends on `mcr-jvm`). New `java/mcr-bridge/` (compiled against `.cache` jars,
  which are never committed). `xtask` gains `fabric.rs`. `mcr-app` gains a `--mods` flag.
