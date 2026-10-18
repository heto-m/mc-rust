# Proposal

## Why

The owner's playtest: under NeoForge, turning the camera has very noticeable input lag and the frame rate is
bad; under Fabric both are fine. Nothing measured Fabric against NeoForge on the same world and view, and nothing
measured the time from a mouse turn to the frame that shows it.

## What Changes

- **Measure first.** Same world, same view, offscreen at 1920x1080: Fabric against NeoForge with an equivalent mod
  set, NeoForge with one trivial mod against its 14-mod set, with and without BetterF3, in the automatic (ahead)
  and lockstep pipelines, with `MCR_MOD_PROFILE=frame`. Numbers go into the design.
- **Turn latency probe.** mcr records each mouse look (time, yaw, pitch) and, after each frame's submit and
  present, which look the rendered camera shows; the latency of a turn runs from the oldest look a frame is the
  first to show to the end of that frame. `--bench` prints p50/p99/max, and windowed runs add it to the
  once-a-second stats line. `MCR_BENCH_MOUSE=<px>` drives the bench through mcr's real mouse path (Rust's
  turn plus Java's input events) instead of turning the player directly.
- **Fix: NeoForge's camera alignment.** NeoForge's patched `Camera.alignWithEntity` turns the camera with
  `setRotation(yaw, pitch, roll)` from `ViewportEvent.ComputeCameraAngles`, which the bridge's `setRotation(FF)`
  arguments never reached. Java's camera kept the shadow player's tick-old look, differed from mcr's, and the
  camera handoff made mcr render terrain from Java's stale camera on almost every turning frame. The bridge now
  seeds the event with mcr's look (listeners still turn on top) and substitutes it in NeoForge's mirrored
  third-person call.
- **Regression check.** An opt-in live JVM test turns mcr's camera every frame under either loader and fails if
  Java hands a camera back; the bench prints handoff frames and turn latency.

## Out of scope

Acquiring the swapchain image before the camera is latched (equal for both loaders and for mcr without mods),
GUI reuse, and the remaining Java frame cost differences between loaders that are within noise.

## Impact

- `java/mcr-bridge`: `CameraMixin` (NeoForge's event and mirrored rotation).
- `crates/mcr-app`: `latency.rs`, the bench's mouse mode and summary, the stats line.
- `crates/mcr-modbridge`: `tests/camera_live.rs`.
- No ABI change: `BRIDGE_VERSION` stays.
- Specs: `mod-entity-render` (camera alignment under NeoForge), `mod-frame-compose` (turn latency). Docs:
  `mod-bridge.md`.
