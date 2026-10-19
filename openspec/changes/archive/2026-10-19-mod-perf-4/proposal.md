# Proposal

## Why

Mod mode has to beat real Minecraft with the same mods on every axis, not only fps. After mod-perf-3 mcr was
ahead on fps and the frame-time tail, but on Medieval Immersed it joined in 28 s against real Minecraft's 26 s and
held about 4.5 GB of RSS against real Minecraft's 3.6 GB peak. Echoes of the Sift joined in 10.4 s against 9.6 s.

## What Changes

- **Connect when Java's reload ends.** Java reported itself loaded only once vanilla's loading overlay was gone,
  which it clears two seconds after the reload. Java's loaded state and the render report now wait for the reload
  itself (`LoadingOverlay.fadeOutStart` set), as vanilla's quick play does.
- **Index pack zips.** `FilePackResources.listResources` walked every entry of a pack zip for each directory it
  listed. A per-zip index sorted by name answers a prefix with a binary search, in the zip's own order.
- **Parallel startup scans.** The mod jar scan and the AOT key's jar hashes run on up to eight threads.
- **Give memory back.** ZGC gets a soft maximum at two thirds of the heap and a five second uncommit delay; the C
  heap is trimmed (`malloc_trim`) once loading activity has stopped; the HUD and skin readers no longer keep the
  join-time `Assets` (a second 127 MB block model cache) alive.
- **Tools.** A sampling heap profiler behind the `heap-profile` feature (`MCR_HEAP_PROFILE`).
- **Present path checked.** winit already picks native Wayland whenever `WAYLAND_DISPLAY` is set and nothing in
  mcr forces X11, so no change.

## Out of scope

Mod-specific Java costs real Minecraft pays too (a HUD mod's per-frame JEI reflection probes), vanilla's serial
block state cache build during bootstrap, and the transient RSS peak during Java's resource reload.

## Impact

- Java bridge: `McrFrame.resourcesLoaded`, `McrRenderExport`, new mixins `LoadingOverlayAccessor` and
  `FilePackResourcesMixin`, new `ZipIndex`.
- `crates/mcr-modmeta`: parallel `scan_dir`. `crates/mcr-jvm`: parallel content hashes, ZGC soft maximum and
  uncommit delay.
- `crates/mcr-app`: `heap_trim`, `heap_profile` (feature), the readers in `play`.
- No ABI change: `BRIDGE_VERSION` stays 22. No wgpu-core changes.
- Specs: `mod-frame-compose`, `modpacks`, `jvm-host`, `long-session-memory`. Docs: `mod-bridge.md`.
