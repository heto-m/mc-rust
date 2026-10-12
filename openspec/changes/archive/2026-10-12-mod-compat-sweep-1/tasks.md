## 1. Sweep

- [x] 1.1 Download about 30 Fabric 26.3 client mods and their libraries into a worktree-private cache
- [x] 1.2 Boot each mod alone through `mcr --mods` against a vanilla server and record the result
- [x] 1.3 Boot all mods together and record the result
- [x] 1.4 Write `docs/mod-compat.md`

## 2. Fixes

- [x] 2.1 Count the loader's MixinExtras as provided, with a resolver test
- [x] 2.2 Autopilot `hover` step for tooltips in Java screens, with a parser test
- [x] 2.3 Run Java from its game dir and log mods left out, with a launch test and a JVM test via the test mod
- [x] 2.4 Java's FOV scale reaches mcr's camera (ABI 8), with a unit test and a live test via the test mod
- [x] 2.5 Accept raw control characters in manifests, with a scanner test
- [x] 2.6 Re-attach Java rendering after a stall, with lockstep tests and a JVM test via the test mod
- [x] 2.7 Policy entry for Particle Rain

## 3. Finish

- [x] 3.1 Update `docs/mod-bridge.md`
- [x] 3.2 Full gate: fmt, clippy, test, java-build, JVM tests
