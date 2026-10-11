## 1. Protocol
- [ ] 1.1 `FrameBegin` message, the Rust posting point in the mod-mode frame loop, and `frame_done` futex wait with the 2 s hang watchdog
- [ ] 1.2 Java: `DeltaTracker.Timer.advanceGameTime` override, partial tick, size application (`Window` accessor, `framebufferSizeChanged`, `SDL_SetWindowSize`)
- [ ] 1.3 Unit tests: watchdog state machine, tick alignment over a simulated 60 s with variable frame times

## 2. Slots and targets
- [ ] 2.1 Slot tagging (`IMMEDIATE`, `GUI`) in `McrCommandEncoder.submit()`, Rust slot queues, submission at `before_frame`/`after_world`
- [ ] 2.2 `ExternalSubmitter` hook in `mcr-render`
- [ ] 2.3 Virtual `MainTarget` textures, per-slot resolution, mod-mode offscreen scene texture plus swapchain blit, dropped color clears, sample/render copy insertion
- [ ] 2.4 Split mcr's frame encoder at slot points (submit mcr work so far, then the Java slot buffers)

## 3. Java world off and mcr HUD
- [ ] 3.1 Mixins: skip `renderLevel`/outline/post effects, skip `LevelExtractor.extract`, cancel the main clear, no-op blit, `present` → `frame_done`
- [ ] 3.2 Hide mcr's native HUD and chat in mod mode; native overlays after the `GUI` slot

## 4. Verification
- [ ] 4.1 Integration test: title screen visible in an offscreen-surface mcr run, with the Mod Menu button text found by pixel probe at its layout position
- [ ] 4.2 Integration test against `xtask server`: HUD drawn, resize to three sizes, Java tick count equals Rust's after 60 s
- [ ] 4.3 Measure render-thread wait per frame (p50/p99) and Java frame time at 1080p; record in design.md
- [ ] 4.4 fmt, clippy, tests, archive
