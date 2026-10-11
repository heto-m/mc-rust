## Context

Facts from the 26.3 sources and jars:

- Mods use the `official` namespace (`Fabric-Mapping-Namespace: official`, no refmaps), so stock `client.jar` plus
  stock Loader works with no intermediary mappings.
- Fabric API 0.162.0+26.3 depends on `fabricloader >=0.19.3` and `java >=25`. Mod Menu 21.0.0 depends on
  `fabric-resource-loader-v1`, `fabric-screen-api-v1`, `fabric-key-mapping-api-v1`, `fabric-lifecycle-events-v1`
  and MixinExtras 0.5.4 or newer.
- No Fabric API module targets `Window`, SDL or renderpearl device classes, so our mixins there do not collide.
  There is no client gametest module to borrow from.

Boot path in vanilla:
1. `net.minecraft.client.main.Main.main` parses jopt options and calls `NativeLibrariesBootstrap.loadLibraries()`.
2. It builds `GameConfig`, names the thread "Render thread", calls `RenderSystem.initRenderThread()`, then
   `new Minecraft(gameConfig)`, then `run()`.
3. The `Minecraft` constructor calls `RenderSystem.initBackendSystem()` (SDL3 `SDL_Init(VIDEO)`), then loops over
   `PreferredGraphicsApi.getBackendsToTry()` with `loadLibrary()` and `createDevice(GpuDebugOptions)`.
4. It then calls `RenderSystem.initRenderer(device)` and `new Window(…, backend, maxSize)`. The window calls
   `backend.createWindow`, and the constructor then calls `device.createSurface(window.handle(), …)`,
   `new SDLEventHandler`, `new SoundManager`, and so on.

## Decisions

**Launch chain.**
- `mcr-modbridge` starts `mcr-jvm` with class path: `mcr-boot.jar`, the Fabric Loader libraries from the meta
  profile, the vanilla libraries from `version.json` (LWJGL, netty, DFU, …) and `client.jar`.
- System properties:
  - `-Dfabric.gameJarPath=<client.jar>`
  - `-Dfabric.side=client`
  - `-Dfabric.addMods=<mcr-bridge.jar><sep><enabled mod jars…>`
  - `-Dorg.lwjgl.librarypath=<natives dir>`
  - `-Dmcr.native.table=<hex address>`
  - `-Djava.awt.headless=true`
- Game args:
  - `--version 26.3 --versionType release --accessToken 0 --offlineDeveloperMode`
  - `--username/--uuid` from the mcr session
  - `--gameDir <data>/java`, whose `mods/` stays empty: mods come from `addMods` so the mod manager controls the set
  - `--assetsDir <.cache/assets>` (the launcher-layout store `xtask assets` already fills, shared with Rust)
  - `--assetIndex <id>`, `--width/--height`
- `McrBoot.main` calls `net.fabricmc.loader.impl.launch.knot.KnotClient.main(args)` (the main class from the meta
  profile). Knot finds the game through `fabric.gameJarPath`, sets up `KnotClassLoader` and Mixin, patches the
  `Minecraft` constructor to run `ClientModInitializer`s, and invokes `net.minecraft.client.main.Main.main`.
  `Main` is not replaced, so mods that hook it still work.
- The native table address travels as a system property, not a static field. Knot may load classes on the
  class path a second time in its own loader, and a string survives that.

**Bridge mod layout.** `java/mcr-bridge/`, plain `javac` against `client.jar`, the loader, sponge-mixin and
MixinExtras from `.cache`. It contains `fabric.mod.json` (`id: mcr_bridge`, `environment: client`, client
entrypoint `dev.mcr.bridge.McrBridge`) and `mcr_bridge.mixins.json` (`compatibilityLevel JAVA_25`, no refmap).
Mixins use `@Inject`/`@WrapOperation` (MixinExtras) rather than `@Overwrite`/`@Redirect` where possible, to coexist
with other mods.

**Mixin targets (this change).**

| Target | Hook | Purpose |
|---|---|---|
| `net.minecraft.client.PreferredGraphicsApi.getBackendsToTry()` | return `{ McrGpuBackend }` | no GL/Vulkan |
| `net.minecraft.client.Minecraft.<init>` call to `VulkanBackend.checkBackendAvailable()` | `@WrapOperation` → null | no Vulkan probe |
| `com.mojang.blaze3d.systems.RenderSystem.initBackendSystem()` | HEAD: `SDL_SetHint("SDL_VIDEO_DRIVER","offscreen")` | fake SDL window |
| `net.minecraft.client.sounds.SoundEngine.loadLibrary()` | cancel at HEAD (`loaded` stays false, vanilla's safe no-sound path) | no OpenAL device |
| `net.minecraft.client.gui.screens.ConnectScreen.connect(Minecraft, ServerAddress, ServerData, TransferState)` | cancel → `McrNet.requestConnect` | no netty socket |
| `net.minecraft.client.Minecraft.runTick(boolean)` | HEAD: `McrFrame.awaitFrame()` | pacing |
| `net.minecraft.client.FramerateLimiter.limitDisplayFPS(int)` | cancel | pacing is Rust's |
| `com.mojang.blaze3d.platform.ClientShutdownWatchdog.startShutdownWatchdog(…)` | cancel | it could halt our process |
| `net.minecraft.client.gui.Gui.setScreen(Screen)` (26.3 moved screen ownership to `Gui`) | TAIL: report class name | lifecycle |
| `net.minecraft.client.Minecraft.crash(Minecraft, File, CrashReport, int)` / `emergencySaveAndCrash(CrashReport)` | HEAD: report crash report path, then park the thread | crash isolation |

**Null GPU backend.** `McrGpuBackend implements com.mojang.renderpearl.api.device.GpuBackend`:
- `loadLibrary`/`unloadLibrary` are no-ops.
- `createWindow` calls `SDL_CreateWindow(title, w, h, flags & ~(OPENGL|VULKAN))` on the offscreen driver, so
  `com.mojang.blaze3d.platform.Window` keeps working unmodified.
- `createDevice` returns `new FrontendGpuDevice(new NullDeviceBackend())`.

`NullDeviceBackend` implements `GpuDeviceBackend` with plain objects that extend `BaseGpuBuffer`/`BaseGpuTexture`:
- `map` hands out a cached direct `ByteBuffer`.
- `compilePipeline` returns a pending that yields a closed-able dummy `BackendRenderPipeline`. The frontend still
  runs shaderc on every shader, which doubles as a shader-compile smoke test.
- The surface does nothing, and passes do nothing.
- `DeviceInfo` reports `isZZeroToOne = true`, `maxTextureSize = 16384`, `minUniformOffsetAlignment = 256`, and
  features matching the eventual wgpu backend.

`mod-render-backend` swaps the null device for the real one behind the same `McrGpuBackend`.

**Pacing.** Rust posts `FrameBegin { index, ticks_to_do, partial_tick, width, height, scale }` in a ring and rings the
doorbell. `awaitFrame` blocks in a downcall (in native, safepoint-safe), then applies size changes and drains the
control ring (shutdown, and later packets and input). In this change the pacer posts frames at 60 Hz with
`ticks_to_do` from a 20 Hz accumulator. The Java side's `DeltaTracker` is not yet overridden; that is done in
`mod-frame-compose`.

**Shutdown.** Rust posts `Shutdown`. Java calls `minecraft.stop()`, `run()` returns, `Main` runs
`exitWorldAndClose()`, the boot call returns, and `mcr-jvm` destroys the VM.

**Lifecycle reporting.** Downcalls `on_mod_loaded(id, version)` (from `FabricLoader.getInstance().getAllMods()` in the
client entrypoint), `on_screen(class)` and `on_crash(path)`. Rust logs them through `tracing` and exposes them for
tests.

## Risks / spikes

- **SDL3 offscreen driver** (spike first): check that LWJGL's SDL3 build includes `offscreen`, that
  `SDL_Init(VIDEO)` works off the main thread on Linux, and what `Window` queries (display modes,
  `MonitorManager`) return. Fallback is the `dummy` driver. On macOS, SDL video must be initialised on the main
  thread. If that fails, a later change replaces `Window`/`SDLEventHandler` with mixins (a no-SDL mode).
- Knot loading classes twice is avoided by passing the table address as a string.
- Shaderc native must load. `NativeLibrariesBootstrap` only preloads it when a Vulkan loader exists, and LWJGL
  loads it lazily otherwise. Verify this.
- Boot heap: Fabric API reaching the title screen needs about 1 GB. Tests use `-Xmx1536m` and run alone.
