## 1. Measure

- [x] 1.1 Equivalent mod sets (NeoForge 14, Fabric counterparts with REI for JEI), a trivial-mod set per loader, and both without BetterF3
- [x] 1.2 Turn latency probe (`latency.rs`), bench mouse mode (`MCR_BENCH_MOUSE`), handoff and stale-frame counts in the bench summary, latency in the windowed stats line
- [x] 1.3 Before numbers: Fabric against NeoForge, automatic and lockstep pipelines, `MCR_MOD_PROFILE=frame`

## 2. Fix

- [x] 2.1 Find the NeoForge-only difference (camera handed back on every turning frame) and its cause (NeoForge's `setRotation(FFF)` in `Camera.alignWithEntity`)
- [x] 2.2 `CameraMixin`: seed `ViewportEvent.ComputeCameraAngles` with Rust's look, give NeoForge's mirrored call Rust's look
- [x] 2.3 Audit the bridge's other optional (`require = 0`) injections against the patched game jar
- [x] 2.4 After numbers, both loaders, both pipelines, trivial and full sets, with and without BetterF3

## 3. Regression check

- [x] 3.1 Unit tests for the latency probe
- [x] 3.2 Opt-in live JVM test `camera_live` for either loader: a turning camera is never handed back
- [x] 3.3 Run it under NeoForge (fails before the fix, passes after) and Fabric

## 4. Wrap-up

- [x] 4.1 `docs/mod-bridge.md`
- [x] 4.2 fmt, clippy, tests; `openspec validate neoforge-perf --strict`; archive
