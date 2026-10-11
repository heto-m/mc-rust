## 0. Spike (go/no-go before section 2)
- [ ] 0.1 Minimal Java program on the `mcr-jvm` host: set `SDL_VIDEO_DRIVER=offscreen`, `SDL_Init(VIDEO)` from `mcr-java`, `SDL_CreateWindow` without GL/Vulkan flags, query size/display; record findings in design.md

## 1. Tooling
- [ ] 1.1 `xtask fabric-fetch`: Fabric meta profile for 26.3 and the pinned loader, libraries with checksums, LWJGL natives from `version.json`, and the pinned test mods (Fabric API, Mod Menu) by sha512
- [ ] 1.2 Unit tests: profile JSON parsing and Maven path construction from fixtures

## 2. Boot
- [ ] 2.1 Scaffold `mcr-modbridge`: class path, system properties and game args builder, with a unit test on the produced command line
- [ ] 2.2 `McrBoot` → `KnotClient.main`, with the table address passed as a system property
- [ ] 2.3 `java/mcr-bridge` mod skeleton: `fabric.mod.json`, mixin config, client entrypoint, compiled by `xtask java-build` against `.cache` jars

## 3. Headless seams
- [ ] 3.1 `McrGpuBackend` plus `NullDeviceBackend`/null surface/null passes
- [ ] 3.2 Mixins: graphics API list, Vulkan probe, SDL offscreen hint, `SoundEngine.loadLibrary`, `ConnectScreen.connect`, shutdown watchdog
- [ ] 3.3 Frame pacing (`runTick` HEAD wait, `FramerateLimiter` off), the Rust pacer, and shutdown messages
- [ ] 3.4 Lifecycle downcalls (mods, screen, crash) and park-on-crash

## 4. Verification
- [ ] 4.1 Integration test (`MCR_JVM_TESTS=1`, `-Xmx1536m`): Fabric API and Mod Menu loaded, initializers ran, `TitleScreen` reported, SDL driver reported as `offscreen`, no OpenAL device and no TCP socket opened (checked via `/proc/self/fd` and an `alcOpenDevice` counter), clean shutdown in under 10 s
- [ ] 4.2 Measure boot time to the title screen and steady idle CPU of the paced Java thread; record in design.md
- [ ] 4.3 `mcr --mods <dir>` wiring, fmt, clippy, tests, archive
