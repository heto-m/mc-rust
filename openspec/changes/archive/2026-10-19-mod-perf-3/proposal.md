# Proposal

## Why

Big Fabric packs are where mod mode has to beat real Minecraft. On Medieval Immersed (102 client mods) mcr ran at
about 650 fps windowed, inside real Minecraft's 640–730 range rather than above it. Nothing measured where Java's
frame went on the Rust side, what caused the spikes, why mcr's RSS was higher, or why the join took longer.

## What Changes

- **Measure first.** Both packs (Medieval Immersed, Echoes of the Sift) on a frozen Fabric server, offscreen
  `--bench-timeline` with `MCR_MOD_PROFILE=frame` and windowed under headless gamescope at 2560x1440, interleaved
  against main. Java profiled with JFR, mcr natively with eu-stackprof, memory with NMT and smaps.
- **Finish Java's encoders off its thread.** wgpu 30 encodes in `CommandEncoder::finish`, the largest Rust cost
  inside Java's frame. Queued encoders now finish on a worker thread, and the render thread takes the buffers when it
  drains.
- **Predict Java's resource packs.** mcr builds its first resource stack with the packs Java's `options.txt`
  selects, so the join no longer reloads every resource at Java's title.
- **Tracker cost.** wgpu-core clears and drops its per-encoder trackers in proportion to the largest resource
  index ever allocated (about 11,000 textures after Java's loading). The tracker patch is kept only if its A/B pays.
- **Tools.** A slow Java frame profile, the steady frame-time tail in `--bench-timeline`, `MCR_JVM_EXTRA` for JVM
  option experiments, swapchain acquire and present time in the stats line, and a clean quit on `SIGTERM`.

## Out of scope

Mod-specific Java costs that real Minecraft pays too (EMF/Fresh Animations, an exception storm in a HUD mod's
JEI probe, ETF's NBT reads), and the JVM's heap footprint (ZGC commits the whole heap).

## Impact

- `crates/mcr-modbridge`: `gpu::finisher`, `backend`, `slots`, `profile`, `compose`.
- `crates/mcr-app`: `respacks::predicted_java_packs` and the shell's first resource load, SIGTERM, the timeline tail, the stats line.
- `crates/mcr-jvm`: `MCR_JVM_EXTRA`.
- `crates/mcr-render`: acquire and present timing.
- No ABI change: `BRIDGE_VERSION` stays 22, no Java changes.
- Specs: `mod-frame-compose`, `mod-registries`, `jvm-host`. Docs: `mod-bridge.md`.
