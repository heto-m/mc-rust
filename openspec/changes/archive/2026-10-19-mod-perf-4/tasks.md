## 1. Profile

- [x] 1.1 Bench setup in the worktree: copied Fabric servers and game dirs on port 25584, main built in its own root
- [x] 1.2 Startup and join: the startup trace, JFR of Java's boot and reload, real Minecraft's join for comparison
- [x] 1.3 Memory: NMT, smaps, `mallinfo2`, a `malloc_trim` probe, a sampling heap profiler for Rust's allocations
- [x] 1.4 Present path: native Wayland under gamescope `--expose-wayland` against Xwayland
- [x] 1.5 Java-side exceptions and probes under JFR

## 2. Fix

- [x] 2.1 Java is loaded when its reload ends, not when the overlay fade is over
- [x] 2.2 Per-zip index for `FilePackResources.listResources`
- [x] 2.3 Parallel mod scan and AOT key hashes
- [x] 2.4 C heap trims after loads
- [x] 2.5 HUD and skin readers hold the resource manager, not `Assets`
- [x] 2.6 ZGC soft maximum and uncommit delay
- [x] 2.7 mimalloc tried and dropped

## 3. Verify

- [x] 3.1 Screenshots of both packs against main
- [x] 3.2 Interleaved A/B against main, offscreen and windowed at 2560x1440, both packs
- [x] 3.3 Format, clippy with `-D warnings`, workspace tests
- [x] 3.4 Design with numbers and what did not pay; `docs/mod-bridge.md`
