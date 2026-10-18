# Design

## Setup

Ryzen 7 5800X, RX 6800 XT (RADV), offscreen 1920x1080, render distance 32. Local vanilla 26.3 server (port
25577, view distance 16, seed `mcrperf`, noon with time and weather frozen) with 6 villagers, 6 cows, a sheep
and an armor stand (`NoAI`) in front of the spawn. Every number comes from one command line:
`mcr --screenshot … --mods <set> --connect localhost:25577 --after-ticks 600 --bench 4000` (30 s warm) with
`MCR_MOD_PROFILE=frame` and `MCR_BENCH_MOUSE=3`, each set in its own game dir with its AOT cache built. The
machine was shared with another agent's builds, so runs alternate and the tables give each run.

Mod sets (copies under the worktree's `run/`, not committed):

| Set | NeoForge 26.3.0.90 | Fabric 0.19.5 |
|---|---|---|
| full | the 14 of `mods-neoforge`: AppleSkin, BetterF3, Chat Heads, Cloth Config, Controlling, EntityCulling, FerriteCore, Freecam, Jade, JEI, MezzConfig, Mouse Tweaks, Searchables, 3D Skin Layers | the same mods' Fabric builds, REI + Architectury for JEI + MezzConfig, plus Fabric API (15 jars) |
| no F3 | full without BetterF3 | full without BetterF3 |
| min | FerriteCore alone | FerriteCore alone |

## Turn latency probe

The offscreen bench used to turn the player directly (`MCR_BENCH_TURN`), which skips the mouse path and never
showed a difference between loaders. `MCR_BENCH_MOUSE=<px>` instead calls the shell's `mouse_motion` every
bench frame, as winit's device events do: Rust turns its player and Java receives the motion as an input event.

`latency.rs` records every look (`Play::look`: time, yaw, pitch). After `draw_with` returns (submitted, and
presented in a window), the yaw and pitch the rendered camera shows are matched against the recorded looks
(1e-3 degrees, wrapped). The newest match and every older look are consumed; the frame's latency runs from the
oldest consumed look to now. A frame that matches nothing while looks wait counts as stale. The bench prints
p50/p99/max with the counts, and the windowed stats line adds the last second's figures. The probe measures to
the present call, not to scan-out; the GPU frame and the swapchain queue come on top and are the same for both
loaders and for mcr without mods.

## Before

| Run | fps | turn latency p50 / p99 / max (ms) | turns shown / stale frames | camera handoff frames | Java frame (ms) |
|---|---|---|---|---|---|
| Fabric full, automatic (ahead) | 1726 | 0.48 / 2.51 / 25.3 | 4000 / 0 | 0 | 0.513 |
| NeoForge full, automatic (ahead) | 1760 | **120.9 / 360.1 / 360.1** | **17 / 3983** | **3999** | 0.504 |
| Fabric full, lockstep | 1131 | 0.80 / 2.89 / 14.3 | 4000 / 0 | 0 | 0.439 |
| NeoForge full, lockstep | 1106 | **111.8 / 465.3 / 465.3** | **19 / 3981** | **3999** | 0.456 |

The frame rate, Java's frame and its phases (pre 0.002–0.003, tick 0.017–0.019, extract 0.124–0.146, world
0.161–0.192, GUI 0.103–0.121 ms), the render thread's wait and the Rust time in Java's downcalls were the same
for both loaders. What differed: under NeoForge, Java handed a camera back on every turning frame, and the
terrain almost never showed the newest turn.

## Root cause

Java's camera alignment runs vanilla's `Camera.alignWithEntity` with Rust's camera swapped into its
`setRotation(FF)` and `setPosition` calls. NeoForge recompiles that method: it builds
`ViewportEvent.ComputeCameraAngles(camera, partial, entity.getViewYRot(partial), entity.getViewXRot(partial),
0)`, posts it, and calls `setRotation(FFF)` with the event's yaw, pitch and roll. The bridge's
`@ModifyArg`s target `setRotation(FF)V` with `require = 0`, so under NeoForge they matched nothing and failed
silently. Java's camera then took the shadow `LocalPlayer`'s look, which Rust writes after each Java tick
(`PlayerShadow.apply`), interpolated by the partial tick: 50 to 100 ms old while the mouse moves.

`McrCameraHandoff.extract` saw a camera that differed from Rust's and reported it as a mod camera, and mcr
rendered its terrain from it in the next frame (the camera handoff for Freecam-style mods). So the whole world
turned with the shadow player's tick-interpolated look: the turn lagged by 50 to 100 ms plus a frame (the
probe's p50 of 112–121 ms), and the view moved in tick-sized steps and lerps instead of per frame, which reads
as a bad frame rate although the frame counter was fine. Fabric runs vanilla's bytecode, where the substitution
lands. The position substitution works on both loaders (NeoForge keeps `setPosition(DDD)`), which is why the
camera only lagged in rotation.

## Fix

`CameraMixin` (no ABI change):

- `@ModifyArg` on the `ComputeCameraAngles` constructor in `alignWithEntity` (yaw, pitch) gives the event Rust's
  look. Its listeners (NeoForge camera mods) turn or roll on top of it, and the change reaches mcr through the
  existing handoff, as Fabric mods hooking `setRotation` do.
- `@ModifyArg` on the second `setRotation(FFF)` (NeoForge's mirrored third-person turn, `yRot + 180, -xRot,
  roll`) gives it Rust's look, as vanilla's mirrored `setRotation(FF)` gets it. Rust's camera already carries the
  mirror.

Both are `require = 0`, absent under Fabric. Rust already owned the camera's orientation: it applies mouse turns
when the event arrives (`Play::look`) and builds the frame's camera at the start of the frame, as vanilla turns
the player in its render loop. With the handoff gone, nothing waits for Java before the turn is drawn: in ahead
mode Java's late frame is reprojected to mcr's current camera (`level_projection`), in lockstep Java renders
from the same camera.

### Other optional injections

The bridge has 13 `require = 0` injections: the camera ones above, the bob-pose `Matrix4f.mul` in
`GameRenderer.renderLevel` and the `RenderSection` compile overloads. A `javap` comparison of the patched and
vanilla `renderLevel` shows the same `mul` call at the same position; the `RenderSection` overloads were handled
in `mod-neoforge-1`. Injections with the default `require` would fail the boot if NeoForge broke them.
(`-Dmixin.debug.countInjections=true` is no audit tool here: it turns every short injection into a fatal
error, the bridge's absent-by-design ones and Freecam's included.)

## After

Same setup, the final round of alternating runs:

| Set, pipeline | Fabric fps | NeoForge fps | Fabric turn p50 / p99 (ms) | NeoForge turn p50 / p99 (ms) | handoff frames |
|---|---|---|---|---|---|
| full, automatic | 1647, 1775 | 1798, 1663 | 0.51 / 2.14, 0.47 / 2.21 | 0.48 / 2.47, 0.52 / 2.16 | 0 |
| full, lockstep | 998, 1086 | 986, 1119 | 0.91 / 2.71, 0.84 / 2.94 | 0.92 / 2.99, 0.82 / 2.79 | 0 |
| no F3, automatic | 1846, 1862 | 1782, 1800 | 0.47 / 2.38, 0.46 / 2.48 | 0.47 / 2.51, 0.47 / 2.46 | 0 |
| min, automatic | 1862, 1911 | 1821, 1741 | 0.47 / 2.07, 0.46 / 2.03 | 0.49 / 2.30, 0.50 / 2.45 | 0 |

Every run shows all 4000 turns with no stale frame. Java's frame, full set, automatic: Fabric 0.541/0.509 ms
(extract 0.156/0.142, world 0.205/0.191, GUI 0.127/0.123), NeoForge 0.489/0.515 ms (extract 0.136/0.148, world
0.184/0.192, GUI 0.115/0.119). NeoForge with FerriteCore alone against its full set: 0.494/0.514 against
0.489/0.515 ms; Fabric 0.480/0.461 against 0.541/0.509 ms. The maxima (5–25 ms on both loaders) are single
hitches of the shared machine; the render thread's wait shows the same maxima.

NeoForge is within 10% of Fabric for the same mod set in every configuration, and so is its turn latency.

- **BetterF3 with F3 open** (full set, automatic): Fabric 967 fps, Java 1.019 ms (extract 0.416, GUI 0.378,
  429 KiB uploaded per Java frame), NeoForge 945 fps, Java 1.045 ms (extract 0.436, GUI 0.375, 425 KiB); turn p50
  0.95 and 0.96 ms. It costs both loaders the same, about half the frame rate.
- **GC** (`-Xlog:gc*`, full set, one run each, ZGC 1536 MiB): Fabric 16 cycles and 66 pauses, NeoForge 13 and 57,
  the longest pause 0.022 and 0.024 ms, no allocation stalls.
- **Pipeline mode**: the bench is uncapped, so automatic mode runs ahead on both loaders; with vsync or a cap it
  stays in lockstep on both (Java's mean frame is far below the 8 ms switch). NeoForge chose no other mode.
- **Third person**: with F5 once (back) and twice (front, NeoForge's mirrored call), both loaders show every
  turn; one handoff frame at the switch to third person back on each loader, which is the perspective handoff by
  design. NeoForge's front run had one 174 ms turn (the first F5 under NeoForge loads its classes then); its p99
  was 4.4 ms against Fabric's 3.8 ms.

## Regression check

`crates/mcr-modbridge/tests/camera_live.rs` (opt-in, `MCR_JVM_TESTS=1 MCR_LIVE_ADDR=…`, `MCR_LIVE_LOADER=neoforge`
for NeoForge) joins a live server, turns mcr's camera 0.6 degrees every frame for 600 frames and fails if Java
hands a camera back. Before the fix under NeoForge: 600 of 600 frames handed back, failed. After: 0 under
NeoForge and Fabric. `latency.rs` has unit tests for matching, stale frames and wrap-around.

## Not done

- **Acquiring the swapchain image before the camera is latched.** In a window with vsync, `get_current_texture`
  can block after the frame's camera was built, so the turn waits up to a refresh more. It is the same for both
  loaders and for mcr without mods; vanilla latches before its swap wait too. A follow-up could acquire first.
- **Windowed confirmation.** All numbers are offscreen; the windowed stats line now carries the turn latency for
  the owner's next playtest.
