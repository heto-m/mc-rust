## 1. Profile

- [x] 1.1 Bench setup in the worktree: copied Fabric servers and game dirs, port 25584, peaceful, noon, frozen ticks, bench spawn and rotation
- [x] 1.2 Java's frame (`MCR_MOD_PROFILE=frame`, JFR through `JAVA_TOOL_OPTIONS`), the bridge's Rust cost by kind, mcr's draw and the present (eu-stackprof)
- [x] 1.3 Spikes: slow-frame profile and the steady frame-time tail in `--bench-timeline`
- [x] 1.4 RSS: NMT and smaps composition; ZGC against G1 and Parallel through `MCR_JVM_EXTRA`
- [x] 1.5 Join: title, ready and windowed join time, and the resource reload at the title

## 2. Fix

- [x] 2.1 Finish Java's queued command encoders on a worker thread
- [x] 2.2 Predict Java's selected resource packs from its options and load them before the title
- [x] 2.3 Quit cleanly on SIGTERM so the AOT cache and preload record survive
- [x] 2.4 Tracker clears and drops in proportion to their members (patched wgpu-core, kept or dropped by its A/B)
- [x] 2.5 Swapchain acquire and present time in the windowed stats line

## 3. Verify

- [x] 3.1 Screenshots of both packs against main
- [x] 3.2 Interleaved A/B against main, offscreen and windowed at 2560x1440, both packs
- [x] 3.3 `cargo fmt`, `clippy -D warnings`, `cargo test --workspace`
- [x] 3.4 Design with numbers and what did not pay; `docs/mod-bridge.md`
