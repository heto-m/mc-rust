## 1. ABI

- [x] 1.1 `BRIDGE_VERSION` 10: `FrameBegin` render camera and handoff flag, frame-state camera, weather flag, shared section-visibility block, with table tests

## 2. Items

- [x] 2.1 Camera handoff: Java reports a mod-moved camera, mcr renders from it, bounded and sanitised, input capture, with unit tests and a live test via the test mod
- [x] 2.2 Weather replacement: detect a cancelled or redirected weather extraction, hide mcr's rain and snow, with a live test via the test mod
- [x] 2.3 Top-down GUI slot without the flip copies, measured before and after
- [x] 2.4 mcr's section visibility feeds Java's block-entity and entity culling, measured in a dense area before and after

## 3. Finish

- [x] 3.1 Re-test Freecam and Particle Rain from the real jars offscreen; update `docs/mod-compat.md`, `mod-policy.json` and `docs/mod-bridge.md`
- [x] 3.2 Merge main, full gate: fmt, clippy, test, java-build, JVM tests
