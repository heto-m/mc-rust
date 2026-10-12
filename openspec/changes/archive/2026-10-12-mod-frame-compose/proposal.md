# Proposal

## Why

`mod-render-backend` proves the backend offscreen. This change puts Java's GUI and HUD into the mcr window: Java
frames must run in lockstep with Rust frames and ticks, Java's draws must land on our frame image in the right order,
and the render thread must never call into Java.

## What Changes

- Frame protocol: Rust posts `FrameBegin` once per frame. It carries the index, `ticks_to_do`, partial tick,
  viewport size and Rust's camera with view and projection matrices.
  - Java consumes it in `awaitFrame`.
  - `DeltaTracker.Timer` is overridden so Java runs exactly Rust's tick count.
  - Window size changes go to Java's `Window` and `framebufferSizeChanged()`.
- Slots: Java command buffers are tagged with the current slot (`IMMEDIATE`, `GUI`; world slots come later).
  - Java's `GpuSurfaceBackend.present()` signals frame done.
  - The mcr render thread submits Java buffers at their slot in FIFO order, waiting for frame N with a hang
    watchdog (2 s, then the bridge detaches).
- Virtual targets: textures created by `MainTarget` become virtual handles that Rust resolves per slot.
  - In mod mode, mcr renders into an offscreen `Rgba8Unorm` scene texture (vanilla's own `MainTarget` format)
    and blits it to the swapchain. In the `GUI` slot, main color is that scene texture and main depth is a private
    depth texture.
  - Clears of virtual color are dropped.
- Java world rendering is off in this change: `GameRenderer.render` skips `renderLevel` and the main-target clear,
  and only the GUI path runs. Menu blur samples mcr's image.
- In mod mode, mcr's native HUD is hidden, because Java's HUD replaces it.

Out of scope:
- Input and screens (`mod-input-screens`).
- Entities and world-space draws (`mod-entity-render-hooks`).
- Pipelined (one-frame-late) GUI.

Acceptance test:
- `mcr --mods` with Fabric API and Mod Menu shows Java's title screen, including Mod Menu's "Mods" button, in the
  mcr window.
- `mcr --mods --server localhost` shows Java's HUD (hotbar, hearts, crosshair, chat) over mcr's world, resizing
  follows the window, and Java's tick counter equals Rust's after 60 s.
- Measured render-thread wait for Java is p99 ≤ 4 ms at 1920×1080 in game, recorded in design.md.

## Capabilities

### New Capabilities
- `mod-frame-compose`: frame lockstep, tick alignment, slots and submission order, virtual targets, GUI
  composition, hang detection.

### Modified Capabilities

## Impact

- `mcr-modbridge` (frame protocol, slots, virtual targets)
- `mcr-render` (external submit hook)
- `mcr-app` (mod-mode frame loop, native HUD toggle)
- `java/mcr-bridge` (mixins)
