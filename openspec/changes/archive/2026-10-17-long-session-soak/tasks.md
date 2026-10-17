## 1. Static audit

- [x] 1.1 Sweep client state, world, net and protocol connection state
- [x] 1.2 Sweep the app's `Play`/`Shell`/world, renderers, audio, UI screens and threads
- [x] 1.3 Sweep GPU buffers, staging and the mesh pipeline
- [x] 1.4 Sweep `mcr-modbridge`, the JVM host and the Java bridge, including `mod-camera-weather`'s shared section block and camera report

## 2. Fixes, each with a churn test

- [x] 2.1 Renderer drops every section on world reset and disconnect, whatever the height range
- [x] 2.2 Reconfiguration and the login after it reset the app's world state like a respawn
- [x] 2.3 Client: clocks and game rules bounded, far chunk sweep, dialog and conduct queues bounded, world-scoped state reset on reconfiguration and dimension change
- [x] 2.4 UI: runtime sprites returned when the server list, HUD heads and overlays drop
- [x] 2.5 Audio: at most 8 decoder threads, menu music events bounded
- [x] 2.6 App: crack decals expire, one pack download thread with stale results ignored, ping slots kept across refreshes, openers reaped
- [x] 2.7 Entity scene: missing-texture keys and equipment misses bounded, idle flat groups dropped
- [x] 2.8 Mod bridge: finished submits pruned, idle pool and upload blocks bounded, server camera held weakly
- [x] 2.9 Net: pooled idle buffer bytes bounded
- [x] 2.10 Audio: decoded sound cache bounded to 64 MiB, glibc's mmap threshold pinned so evicted buffers leave the heap

## 3. Soak harness

- [x] 3.1 wgpu internal counters and entity layer counts exposed
- [x] 3.2 `mcr --soak`: offscreen, rejoin, respawn, stats lines
- [x] 3.3 `cargo xtask soak`: server, console churn, sampling, verdict
- [x] 3.4 Baseline 20-minute soak on main plus the harness
- [x] 3.5 20-minute soak after the fixes, flat
- [x] 3.6 10-minute mod-mode soak with Fabric API and Mod Menu, JVM heap bounded
- [x] 3.7 Before and after numbers in design.md

## 4. Finish

- [x] 4.1 Merge main, validate and archive the change
- [x] 4.2 fmt, clippy and the workspace tests
