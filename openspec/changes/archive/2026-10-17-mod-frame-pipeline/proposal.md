# Proposal

## Why

With the 29 mods in the owner's play set, mcr runs at about 350 fps. Without mods it runs at about 2000 fps. Every
mod-mode frame is serial. mcr posts a frame to Java, encodes its opaque terrain, and then waits until Java has
ticked, extracted and recorded the whole frame. Only then does it submit Java's work and present. Nobody had
measured where that time goes: in Java's ticks, in its world and GUI rendering, in the Rust GPU calls Java makes,
or in mcr's submits. Nobody had measured which mods cost the most either.

## What Changes

- **Frame profile.** Java publishes its frame phase times with every frame: input and packets, ticks, extract,
  world, GUI and the rest of the frame. mcr adds its wait for Java, its submits of Java's work, and the time
  Java's GPU downcalls spend in Rust. With `MCR_MOD_PROFILE=1`, mcr logs a profile line once a second. Java
  samples its render thread through JFR and logs a per-mod share every 10 seconds. `mcr --screenshot … --bench N`
  now also works in the menu and shell flow, and it prints offscreen fps and the profile.
- **One frame ahead.** As soon as Java finishes a frame, mcr can post the next one. Java then renders frame N+1
  while mcr composes frame N, draws its own world and presents. The automatic mode does this when mcr's frame
  rate is uncapped (no vsync and no fps limit), or when Java's frames take over 8 ms. It keeps lockstep otherwise.
  `MCR_MOD_PIPELINE=lockstep|ahead|auto` overrides the mode. Ticks, slot order, hang detection and bounded waits
  are unchanged. A frame composed one mcr frame late gets its level projection rewritten for mcr's current
  camera, so Java's entities stay on mcr's terrain while the camera moves.
- **Cheaper GPU calls.** Fence waits and polls stop doing work Java cannot observe. A fence blocks only while a
  readback depends on it, because host writes already reach the GPU in queue order. A zero-timeout poll of the
  open recording's fence no longer forces a submit. The readback poll skips `device.poll` when nothing is
  pending. The render thread no longer holds the slot queue lock while it submits.
- **Fewer command buffers.** Slots that mcr submits at the same point share one command buffer. A queued item
  with nothing to execute skips `queue.submit`. In game, this saves one `finish()` and two submits per frame.
- ABI: `BRIDGE_VERSION` 12. It adds frame-state phase counters (`FSTATE_PROF`, `PROF_*`) and the
  `D_LEVEL_VIEW` device op.

## Impact

- `crates/mcr-modbridge`: `compose.rs` (pacing and the late-camera rewrite), `frame.rs` (`Pipelining`,
  `AheadPolicy`), the new `profile.rs`, `gpu/slots.rs` (frame-bounded drains, projection patch), and
  `gpu/backend.rs` and `gpu/ffi.rs` (fences, polls, downcall timing).
- `java/mcr-bridge`: `McrFrameTimes` and its hooks, `McrProfiler` (JFR), and the `FrameSlots.levelView` report.
- `crates/mcr-app`: the pacing hint from vsync and the fps cap, the profile line, and the shell bench.
- Specs: `mod-frame-compose` (lockstep, pipelining, late-camera rewrite, profile) and `mod-render-backend`
  (fences).
