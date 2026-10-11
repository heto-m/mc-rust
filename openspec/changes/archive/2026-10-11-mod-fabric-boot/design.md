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
- Knot loading classes twice: resolved with `fabric.systemLibraries` (see the implementation notes). Only the
  bridge table address travels as a string.
- Shaderc native must load. `NativeLibrariesBootstrap` only preloads it when a Vulkan loader exists, and LWJGL
  loads it lazily otherwise. Verify this.
- Boot heap: Fabric API reaching the title screen needs about 1 GB. Tests use `-Xmx1536m` and run alone.

## Spike 0.1: SDL3 offscreen on `mcr-java` (go)

A throwaway `main(long, String[])` class booted on the `mcr-jvm` host with only LWJGL core, SDL and shaderc
on the class path and `-Dorg.lwjgl.librarypath` set to the extracted natives:

- LWJGL 3.4.3's SDL 3.4.14 build ships `wayland x11 kmsdrm offscreen dummy evdev`. With the
  `SDL_VIDEO_DRIVER=offscreen` hint, `SDL_Init(VIDEO)` succeeds on `mcr-java` (Java names that thread `main`)
  in about 11 ms, with no display connection, although this machine runs Wayland.
- The offscreen driver reports one display (1024×768, refresh 0 Hz, no fullscreen modes).
  `SDL_CreateWindow(854, 480, RESIZABLE | HIGH_PIXEL_DENSITY)` works. Size and pixel size are 854×480 at
  scale 1.0, the position is 85,144, and `SDL_SetWindowSize` + `SDL_SyncWindow` resizes. Six events are queued.
- shaderc loads lazily from `org.lwjgl.librarypath` (`shaderc_compiler_initialize` works), so it needs no
  Vulkan-loader preload.
- While video is initialised, SDL holds two D-Bus session connections (unix sockets; they disappear with
  `DBUS_SESSION_BUS_ADDRESS=disabled:`). No TCP. Accepted; setting the variable in-process would race
  Rust threads.
- In the real boot, vanilla's `Window`, `MonitorManager` and `SDLEventHandler` run unmodified on this driver
  (log: "Created window using SDL video driver: offscreen"). Only `setIcon` warns ("not supported").

No-SDL mode is not needed on Linux. macOS (video on the main thread) is still unverified.

## Implementation notes and deviations

**Bridge entries: a second table (decided).** The host table stays static and host-only (version 1,
unchanged). `mcr-jvm` gained `ExtensionTable::leak(version, entries)`. It builds a table with the host
header layout, magic `MCRX` and an embedder-chosen version, and leaks it, because Java may hold the address
until the process exits. Java maps either table through the new `dev.mcr.boot.NativeTable`. The bridge table
(`BRIDGE_VERSION` 1) goes to Java as `-Dmcr.bridge.table=<hex>`, not through `host_call`: the mod needs it
before any host op could serve it, and a string survives any class loader. Entries:
`shared_address (I)J` (critical; control ring and doorbell by id) and `report (IJI)V` (normal; kind plus
UTF-8). Rust unit tests parse `BridgeAbi.java` against the Rust list and constants, like `Abi.java`.

**One `report` downcall** instead of `on_mod_loaded`/`on_screen`/`on_crash`. The kinds are `MOD_LOADED`,
`INIT`, `SCREEN`, `CRASH`, `INFO` (`key=value` diagnostics), `CONNECT` and `STOPPING`. Rust clamps the length
to 16 KiB, decodes lossily, never unwinds into Java, and queues events in a bounded channel (1024, with a
drop counter). Crash state lives in an atomic, so dropping events cannot hide a crash.

**Class loaders.** `mcr-boot.jar` is passed as `-Dfabric.systemLibraries`, so Knot delegates `dev.mcr.boot.*`
to the application loader and the host `Native` state exists once. The design's
`-Dmcr.native.table` property is therefore not needed and not passed. The test asserts that `McrBridge` comes
from `KnotClassLoader` and `Native` from `AppClassLoader`. Rule for later changes: Rust never resolves
Minecraft or mod classes by name (`FindClass` and `StaticVoidMethod::lookup` see the boot or system loader
only). Rust→Java calls are ring messages or upcall stubs that Knot-loaded code creates and registers. A bridge
`register_upcalls` entry will be added to the bridge table when the first bridge upcall exists.

**`System.exit` and crashes.** `Bridge` watches `JavaVm::join` on a thread. `VmOutcome::Exited` becomes
`BridgeEvent::VmExited` and `ShutdownOutcome::Exited`. A crash (`Minecraft.crash`/`emergencySaveAndCrash`
HEAD: save the report, report its path, park in a native doorbell wait) becomes `ShutdownOutcome::Crashed`.
Both return `needs_process_exit()`. `mcr`'s `ModHost::finish` then calls `std::process::exit` and never
`release_exit`. The opt-in test covers both paths in child processes.

**Stdout.** Vanilla's `Bootstrap.shutdownStdout()` (called from `saveReport`) closes Java's `System.out`,
which is file descriptor 1 of the whole `mcr` process. The crash child found this: its Rust `println!` was
silently lost. `BootstrapMixin` cancels it.

**No name resolution in Java.** Mod Menu's update checker and vanilla's services discovery both try HTTPS at
boot, even with `--offlineDeveloperMode`. Java gets `-Djdk.net.hosts.file=<gameDir>/mcr-hosts` (localhost
only), so every remote lookup fails and no TCP socket is opened ("Failed to connect", "Error checking for
updates" in the log). This also blocks vanilla skin and texture HTTP later; see the risks.

**Extra mixins.**
- `MessageBox.error`/`errorWithContinue` are cancelled and reported. SDL message boxes can fall back to
  zenity or kdialog and block the render thread.
- `Library.createDeviceTracker` returns a polling tracker over `DeviceList.EMPTY`, and `DeviceList.query`
  returns `EMPTY`. Enumerating devices would connect to PipeWire/Pulse even without `loadLibrary`.
- `Library.tryOpenDevice` is the `openal_open_device` probe the test counts.
- The Vulkan probe is an `@Inject` HEAD on `VulkanBackend.checkBackendAvailable` (returns `null`), not a
  `@WrapOperation` in `Minecraft.<init>`. Nothing in this change needs MixinExtras, so the mod compiles
  without it.
- Every hook is `@Inject` (no `@Overwrite`/`@Redirect`).

**Launch details.**
- Extra properties: `-Dfabric.noGui=true` (Fabric's error GUI would spawn a process), `-Djava.library.path`
  (jtracy), `-Djna.tmpdir` and `-Dio.netty.native.workdir` under the game dir, and
  `--add-exports=java.base/jdk.internal.misc=ALL-UNNAMED` (vanilla's JVM args).
- The profile's `-DFabricMcEmu= net.minecraft.client.main.Main ` is passed through.
- `McrBoot.main(table, ["launch", <main class>, args…])` reflectively calls `KnotClient.main` on `mcr-java`.
- `prepare_game_dir` writes `options.txt` with `onboardAccessibility:false` only when the file is missing.
  Otherwise the accessibility onboarding screen comes before the title screen.

**Tooling.**
- `fabric-fetch` writes `.cache/fabric/launch.json` (format 1, cache-relative paths). `mcr-modbridge`
  reads it, so the classpath logic lives in one place.
- Libraries go to `.cache/fabric/libraries/<maven path>`. The loader has no checksum in the profile, so the
  maven `.sha512` sidecar is used.
- Natives are extracted flat into `.cache/fabric/natives/<os>-<arch>`.
- Test mods are pinned by Modrinth URL and sha512 into `.cache/mods-test/`.
- `java-build` learned `deps.txt` (`module <name>`, `fabric-launch`), `resources/`, `MANIFEST.MF`
  (`Fabric-Mapping-Namespace: official`), `-proc:none` and `-Xlint:-classfile`.

**Stale jars.** `mcr_jvm::stale_java_jar(module_dir, jar)` returns any file under `java/<module>` newer than
the jar. Both opt-in tests fail with "run `cargo xtask java-build`" instead of booting old code.
`java-build` touches an up-to-date jar, so a touched but identical source cannot wedge the check.

**Pacing.** A `mcr-bridge-pacer` thread posts `FrameBegin` (32 bytes, native endian) at 60 Hz with a 20 Hz
`TickClock` (at most 10 ticks per frame) and skips frames while the 64 KiB ring is full. `awaitFrame` drains
everything, keeps the newest frame and sums the ticks. `Bridge::set_paused` stops posting, for tests and the
later pause flow. Shutdown is a ring message the pacer pushes last (retrying for up to 5 s).

**`mcr --mods <dir>`.** It boots every `*.jar` in the directory next to the native client, with game dir
`<--game-dir>/java`, and prints bridge events to stderr. On exit it shuts the bridge down, and it exits the
process when the VM exited or crashed. No window or input integration yet.

## Measurements (task 4.2)

Ryzen 7 5800X, Linux 7.2, Temurin 25.0.4.1, `-Xmx1536m`, ZGC, Fabric API + Mod Menu (52 mods counting
Fabric API modules), the opt-in `fabric_boot` test in the debug test profile (dependencies at opt-level 3):

| Metric | Value |
|---|---|
| `Bridge::start` → `TitleScreen` reported | 7.0–7.2 s |
| Shutdown request → VM destroyed | 351 ms |
| Render thread CPU at 60 Hz on the title screen (null device) | 40 ms/s (4 % of a core) |
| Render thread CPU with frames paused | 0.000 ms/s |
| Process RSS at the title screen (test process) | 1820 MiB, peak 1837 MiB |
| Java heap used / committed / max at the title screen | 750 / 1382 / 1536 MiB |

The heap figure is not a live set. Generational ZGC grows the heap toward `-Xmx` during the resource reload,
so RSS follows the heap cap rather than the live data. `mod-manager-ui` should size `-Xmx` from the mod count.
A lower cap and `-XX:SoftMaxHeapSize` are worth measuring before choosing defaults.

## Risks for `mod-net-bridge`

- The hosts-file guard blocks all Java DNS. Skins (`textures.minecraft.net`), server resource packs fetched
  by Java and mods with web APIs will fail until Rust either proxies those fetches or the guard becomes an
  allow-list. Decide which HTTP Java may do, and keep netty's game socket out of reach. Literal IP addresses
  are not blocked; only `ConnectScreen.connect` is intercepted today.
- `ConnectScreen.connect` currently only reports `CONNECT`. The `BridgeChannel`/`EmbeddedChannel` seam
  (`Connection.setupInboundProtocol` spike) is untested.
- Bridge upcalls will need a `register_upcalls` entry in the bridge table, registered from Knot-loaded code.
  The host's thread and downcall-scope checks apply unchanged, because the render thread is `mcr-java`.
- Packets need Rust→Java data rings beyond the 64 KiB control ring. Its frame messages must not starve
  behind packet bursts, so use separate rings.
- `awaitFrame` is the only point where Java drains Rust messages. Packet handling must happen there or in
  `runTick`, never on netty threads.
- `Minecraft.crash` parks the render thread forever. A crash while connected leaves Rust with a dead Java
  side; the network bridge must then stop forwarding and keep the native client running.
