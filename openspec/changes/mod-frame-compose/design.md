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
