## Context

Vanilla's per-frame path is `Minecraft.runTick(boolean)`:
1. `packetProcessor.processQueuedPackets()`, `runAllTasks()`.
2. `tick()` × `min(10, deltaTracker.advanceGameTime(now))`.
3. `soundManager.updateSource(camera)`, `mouseHandler.handleAccumulatedMovement()`.
4. `renderFrame`:
   - `windowSurface.acquireNextTexture()`
   - `gui.update`, `gameRenderer.update`
   - `gameRenderer.extract(deltaTracker, advance)`: window, options, camera, `LevelExtractor.extract`,
     `gui.extractRenderState`
   - `gameRenderer.render()`: clear main, `renderLevel()`, post effects, clear depth, `guiRenderer.render()`
   - `windowSurface.blitFromTexture(encoder, mainRenderTarget().getColorTextureView())`, `encoder.submit()`,
     `windowSurface.present()`

The main target is `com.mojang.blaze3d.pipeline.MainTarget` (`RGBA8_UNORM` + `D32_FLOAT`). The GUI clears depth and
uses pipelines without depth state. `GuiRenderer.draw` runs `executeDrawRange` for draws before the blur, then
`gameRenderer.processBlurEffect()`, then draws after the blur.

mcr's main thread runs winit, a 20 Hz tick accumulator and rendering (`mcr-app` `shell.rs`/`play.rs`). Its
renderer uses reverse-Z infinite projection, like vanilla's reverse-Z (which is finite).

## Decisions

**Lockstep, not pipelining (v1).**
1. At the start of a frame, the render thread posts `FrameBegin { index, ticks_to_do, partial_tick, width, height,
   camera_pos (f64×3), yaw, pitch, view (mat4), proj (mat4), flags }` and rings the doorbell.
2. It then encodes its own world.
3. At the `GUI` slot it waits for Java's `frame_done(index)`.

GC cannot stall us for long (ZGC, under 1 ms), and Java's frame has no terrain work. Pipelining (showing GUI N-1)
is a later option if measurements require it. The wait is bounded by a 2 s hang watchdog. On expiry, the bridge
is marked hung, Java buffers are no longer awaited, mcr's native HUD returns, and the user gets a toast. The render
thread never calls into Java: it only waits on a futex that the Java thread signals from a downcall.

**Tick alignment.** Rust's accumulator decides `ticks_to_do`. A mixin on `DeltaTracker.Timer.advanceGameTime(long)`
returns Rust's value and sets the partial tick (`getGameTimeDeltaPartialTick`) from `FrameBegin`. The ten-tick cap in
`runTick` is identical on both sides. Pause state stays Java's (`Minecraft.pause`), but on a multiplayer server it is
always false.

**Size.** On a size change, Java writes the framebuffer and window sizes into `com.mojang.blaze3d.platform.Window`
through an accessor, then calls `Minecraft.framebufferSizeChanged()` (the `WindowEventHandler` callback that resizes `MainTarget` and recomputes the GUI scale). Scale is
Java's own option, since GUI authority is Java's. The SDL offscreen window is resized too (`SDL_SetWindowSize`), so
mods that query SDL agree.

**Slots and submission.**
- The Java bridge keeps a current slot: `IMMEDIATE` by default (resource uploads, reloads), and `GUI` between hooks
  at HEAD and TAIL of `GuiRenderer.render()`.
- `McrCommandEncoder.submit()` finishes the wgpu encoder and pushes `(frame index, slot, CommandBuffer)` to a Rust
  queue instead of submitting.
- The render thread drains `IMMEDIATE` buffers before its own work every frame (FIFO, never dropped), and submits
  the `GUI` buffers of the matching frame after its world passes and before native overlays.
- Buffers of a stale frame cannot occur in lockstep. If they ever do, they are submitted anyway, because the
  targets are frame-invariant textures, and logged.

**Virtual targets.**
- HEAD/TAIL hooks around `MainTarget`'s texture creation (constructor and `resize`) set a thread-local flag, and the
  backend returns `VirtualTexture{MainColor|MainDepth}` handles of the requested size.
- Rust resolves them when a pass descriptor references them:
  - `GUI` slot: `MainColor` → mcr's scene texture. `MainDepth` → a private
    `Depth32Float` texture of the viewport size.
  - Sampling `MainColor` (blur, screenshots) reads that same image. Vanilla's blur post chain copies into its own
    targets first, so no feedback loop occurs. A pass that would both sample and render `MainColor` gets a copy
    inserted.
- Clears of `MainColor` are dropped; depth clears apply.
- `GameRenderer.render`'s initial `clearColorAndDepthTextures(main…)` is cancelled by a `@WrapOperation`.
- Scene texture: mcr normally renders straight into a non-sRGB view of the swapchain, with gamma-space shaders like
  vanilla (`mcr-render` `surface_format`). In mod mode it renders into an offscreen `Rgba8Unorm` +
  `Depth32Float` pair instead, which `mcr-render` already builds for headless runs, and blits it to the swapchain
  at the end. That costs one fullscreen copy. In exchange, Java pipelines' declared `RGBA8_UNORM` target matches
  exactly (no per-format pipeline variants), blending matches vanilla's gamma-space look, and the scene can be
  sampled (blur, screenshots), which swapchain images cannot.

**Java world off.**
- A mixin on `GameRenderer.render()` skips the `renderLevel()` branch, `levelRenderer.blitEntityOutline()` and
  `applyPostEffects()`.
- `GameRenderer.extract` still runs `extractCamera`, but `LevelExtractor.extract` is skipped until
  `mod-entity-render-hooks`.
- `blitFromTexture` is a no-op, because the GUI already drew into mcr's image. `present()` → `frame_done`.

**mcr side.**
- Mod mode hides mcr's native HUD and chat overlay. Debug overlays and native overlays (mod manager, toasts about
  the bridge) draw after the `GUI` slot.
- `mcr-render` gets a small `ExternalSubmitter` hook: at the named points (`before_frame`, `after_world`; later
  the world slots of `mod-entity-render-hooks`), the renderer finishes and submits its encoder so far, calls the
  hook with the queue, and continues on a fresh encoder. The existing `Overlay` trait records into mcr's own
  encoder and stays for native overlays.

## Risks

- Splitting mcr's encoder into several submits per frame costs a little CPU per submit. Measured in task 4.3.
- GUI blur correctness depends on main color holding the world at that point, which is true because the slot runs
  after mcr's world passes.
- Java-side frame cost of the HUD in mod-heavy setups: measured, and pipelining is the escape hatch.

## As built

Where the implementation departs from the decisions above:

- **Main target is real, and flipped in and out.** Renderpearl targets are bottom-up (GL row order) and
  mcr's scene is top-down, so `MainTarget`'s color can't simply alias the scene. `MainTarget` textures are created
  with a bridge-only `TEXTURE_VIRTUAL` usage bit (set by `MainTarget.allocateAttachments` and
  `RenderTarget.createBuffers` mixins). At the GUI slot, the compositor (`compose.rs`):
  1. copies the scene into Java's main color, flipped;
  2. submits Java's queued buffers;
  3. copies the result back, flipped.

  That costs two fullscreen passes per frame. Color clears of virtual textures, `E_CLEAR` and pass load clears
  alike, are dropped, so `GameRenderer.render`'s main clear needs no mixin. A pass that samples the target it
  renders into reads a copy taken just before it.
- **Slots by cutting the encoder.** Vanilla submits once per frame, after `GuiRenderer.render`. A new `E_SPLIT`
  encoder op cuts the recording at `GuiRenderer.render` HEAD and RETURN. It queues what was recorded so far,
  with its staged uploads, under the current slot, and leaves fences, readbacks and Java's transient memory to
  the real submit. While composing, every Java submit is queued (`gpu/slots.rs`) and runs on the render thread
  in FIFO order, inside a validation error scope. A fence wait on queued work returns false instead of
  blocking, so Java can never deadlock against the render thread.
- **Bounded waits.** The render thread waits for Java's frame for at most 100 ms once Java has loaded (50 ms
  while loading), not the full 2 s. A late frame is presented without Java's newest GUI:
  - in game, mcr's live world is shown;
  - while loading, the last composite is shown, so the loading screen stays up.

  The 2 s hang deadline (60 s while loading) still detaches the bridge. A hung mod therefore costs at most one
  100 ms frame, never a 2 s freeze (`compose_hang` test).
- **Ticks.** `TickBank` hands every Rust tick to the next posted frame. Java runs at most 10 per frame and
  carries the rest, so no tick is lost. Ticks before Java's first frame are dropped, because Java has no game
  to tick yet. `Minecraft.tick` HEAD counts Java's ticks into the shared frame state.
- **Connecting.** `mcr --mods --connect` defers the join until Java has loaded and its GUI is composed. Java
  processing play packets mid-reload threw in `ParticleEngine` and disconnected the session.
- **Not done here.**
  - `SDL_SetWindowSize` is called for mods that query SDL, but SDL's resize events are left to vanilla.
  - The GUI scale stays Java's own option.
  - Java's `logs/` directory lands in mcr's working directory, as it did before this change.

## Measurements

Ryzen 7 5800X, RX 6800 XT (Vulkan). Times are from leaving `awaitFrame` to `frame_done`, for Java's frame, and
the render thread's blocking wait at `after_world`.

| Run | Frames | Render-thread wait p50 / p99 / max | Java frame p50 / p99 / max |
|---|---|---|---|
| `mcr --mods --connect` at 1920×1080, 30 s in game (real terrain, uncapped fps) | 14 760 | 0.18 / 1.84 / 21.8 ms | 0.38 / 2.06 / 22.1 ms |
| `compose_live` at 1920×1080, 20 s in game (sky only, 60 fps) | 1 233 | 0.19 / 2.33 / 3.2 ms | 0.39 / 2.61 / 3.7 ms |

p99 is under the 4 ms target. The in-game runs had no late frames. Java's and Rust's tick counters matched
exactly (707 = 707 and 1267 = 1267). mcr's whole frame in the first run, including the encoder split and both
flips, took 0.90 ms p50 and 2.81 ms p99 of CPU time.
