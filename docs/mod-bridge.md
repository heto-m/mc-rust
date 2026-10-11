# Mod bridge architecture

Design B (hybrid), Fabric first. The `mcr` process embeds a Java 25 JVM that runs the stock 26.3 `client.jar`
under Fabric Loader (Knot) with the user's mods and our bridge mod. Rust stays the real client: it owns the window,
the socket and protocol state, terrain, sky, weather and audio output. Java is a *shadow client*: it gets a copy of
the server packet stream, ticks its own `ClientLevel` so mods see a normal game, and draws GUI, HUD, entities and
mod overlays through a renderpearl backend that records wgpu work against our device.

Scan data and the decision: `.cache/modscan/ANALYSIS.md` (gitignored). Changes: `openspec/changes/mod-*`.

```
 Rust (mcr process)                                      |  JVM (same process, thread "mcr-java")
                                                         |
 main thread: winit + mcr render loop                    |  Knot -> net.minecraft.client.main.Main
   frame N:                                              |    Minecraft.run() paced by bridge
   1 post FrameBegin(N, camera, ticks, input) ---ring--->|    awaitFrame (blocking downcall, in native)
   2 encode opaque terrain                               |    drain packets -> EmbeddedChannel -> Connection
   3 submit Rust CB                                      |    tick() x ticks_to_do (ClientLevel, mods)
   4 submit Java CB slot WORLD_SOLID      <--handover----|    extract + render: renderpearl calls
   5 submit Rust translucent/clouds/weather              |      -> McrGpuBackend (Java) -> FFM downcalls
   6 submit Java CB slot WORLD_AFTER / HAND <--handover--|      -> mcr-modbridge (Rust) records wgpu CBs
   7 submit Java CB slot GUI             <--handover----|    surface.present() -> frame_done(N)
   8 native overlays (debug, mod manager), present       |
                                                         |
 I/O threads: socket, crypto, compression, decode        |
   clientbound frames (decompressed) --------copy-ring-->|  BridgeChannel inbound (one frame per msg)
   serverbound <--filter by (phase, id)---------ring-----|  BridgeChannel outbound (tagged with phase)
                                                         |
 mcr-audio <------------------ sound command ring -------|  Library/Channel mixins (SoundEngine kept)
```

## Process and threads

- **Embedding.** `mcr-jvm` loads `libjvm` at runtime (`libloading`, `jni-sys` types) from a located JDK 25
  (`MCR_JAVA_HOME`, `.cache/tools/jdk25`, `JAVA_HOME`) and calls `JNI_CreateJavaVM` on a dedicated 16 MiB-stack
  thread (never the primordial thread). JNI is used only to boot: `FindClass` + `CallStaticVoidMethod` on
  `McrBoot.main(long table, String[] args)`. Flags: `-Djava.class.path=<boot jar + loader + libs + client.jar>`,
  `--enable-native-access=ALL-UNNAMED`, `-Xrs`, `-XX:+UseZGC`, `-XX:+UseCompactObjectHeaders`, `-Xmx` from
  settings (default 2G), `-Dfabric.gameJarPath`, `-Dfabric.addMods`, `-Dorg.lwjgl.librarypath`, plus `exit`/`abort`
  hooks so `System.exit` from a mod becomes an orderly mcr shutdown.
- **Java→Rust** calls are FFM downcalls to function pointers Rust passes in a versioned `#[repr(C)]` table
  (no symbol export needed on any OS). **Rust→Java** upcall stubs are registered by Java at boot but are only ever
  invoked on the Java thread inside a downcall. Every other Rust→Java flow is a message in an SPSC ring that
  Java drains, with a futex/condvar doorbell. Bulk data (packets, draw streams, PCM) lives in Rust-owned
  off-heap memory viewed from Java as `MemorySegment`s, so nothing is copied per call.
- **Threads.** The Java thread runs Minecraft's "Render thread" (`Main.main` → `Minecraft.run()`), paced by a
  blocking `awaitFrame` downcall. The mcr main thread never enters Java, so safepoints never stop it. A GC
  pause only delays the Java frame, and the render thread waits for that frame's slots (ZGC pauses are under 1 ms).
  The JVM cannot be re-created in-process, so changing the mod set requires restarting `mcr`.

## Seams (vanilla 26.3 classes)

| Seam | Hook | Owner |
|---|---|---|
| GPU | `PreferredGraphicsApi.getBackendsToTry` → `McrGpuBackend` implementing `com.mojang.renderpearl.api.device.GpuBackend`; `createDevice` returns `new FrontendGpuDevice(new McrDeviceBackend())` (`GpuDeviceBackend`, `CommandEncoderBackend`, `RenderPassBackend`, `GpuSurfaceBackend`) | Java records, Rust submits |
| Shaders | Frontend `GlslCompiler` already emits SPIR-V (shaderc). The backend gets `SpvModule` plus flattened bindings (set 0, binding = uniform index) → Rust splits combined image samplers → naga `spv-in` → wgpu | Rust |
| Window | SDL3 `offscreen` video driver (hint set in `RenderSystem.initBackendSystem`), so `Window` works on a fake window. Size and scale are pushed per frame | Rust |
| Input | Drained at `SDLEventHandler.pollEvents` into `KeyboardHandler.keyPress/charTyped`, `MouseHandler.onButton/onMove/onScroll`. `InputConstants.isKeyDown/grabMouse/releaseMouse` and `ClipboardManager` are redirected | Rust |
| Network | `Connection.connect` is redirected to a `BridgeChannel` (netty `EmbeddedChannel`) carrying vanilla codecs, so `ConnectScreen` runs unchanged while Rust connects. Outbound packets are filtered by (phase, packet class) | Rust owns socket and state |
| Player | Rust's local player and camera overwrite `LocalPlayer`/`Camera` every tick and frame. Vanilla gameplay keybinds are suppressed in Java | Rust |
| World | Java keeps its own `ClientLevel`/`ClientChunkCache`. Terrain compile and draw are disabled (`LevelRenderer.compileSections`, `ChunkSectionsToRender.renderGroup`) | duplicated, see below |
| Sound | `SoundEngine` logic kept. `Library.init`/`acquireChannel` and `Channel`/`Listener` methods forward to `mcr-audio` | Rust plays |

## Frame composition

Java's `MainTarget` color and depth become *virtual targets*: the backend resolves them to Rust textures per
**slot** (`WORLD_SOLID`, `WORLD_TRANSLUCENT`, `WORLD_AFTER`, `HAND`, `GUI`). In mod mode, mcr renders into an
offscreen `Rgba8Unorm` + `Depth32Float` scene. That is exactly vanilla's `MainTarget` format and gamma-space look,
and it is blitted to the swapchain at the end. World slots alias that scene color and its reverse-Z depth. Vanilla
is reverse-Z too, and Java uses Rust's projection matrix. The GUI slot aliases the same color after mcr's world
passes, so `GuiRenderer` draws straight onto our image and the menu blur samples our world. Java records each slot into its own wgpu command buffer on the Java thread. The mcr render thread submits
them in slot order between its own passes (frame lockstep). Clears of virtual targets are dropped. In mod mode, Java
renders entities, block entities, the hand and mod world overlays. Rust renders terrain, sky, clouds, weather,
water and native particles.

## World duplication

Java's `ClientLevel` parses every chunk again (`ClientChunkCache.replaceWithPacketData` → `LevelChunk`, `PalettedContainer`,
`DataLayer` light). At render distance 12 this costs about 60–100 MB of heap plus the decode CPU on the Java
thread. Accept it for now. Later, back `LevelChunkSection` storage with a `BitStorage` implementation over Rust's
off-heap section arrays (`BitStorage` is an interface), if measurements say it pays.

## Policies

- Mod mode is opt-in (mods folder non-empty and enabled). Without mods, no JVM starts.
- With mods, Java has GUI authority: screens, HUD and chat come from Java. Native screens remain for mcr-only UI
  (mod manager, debug, crash fallback). On a Java crash, mcr detaches the bridge and continues natively.
- Rust answers every vanilla protocol acknowledgement. From Java it forwards only custom payloads, custom query
  answers, container and screen actions, and chat/commands (re-signed by Rust).
- Performance and renderer mods that need OpenGL (Sodium, Iris, ImmediatelyFast, Distant Horizons, Xaero's,
  JourneyMap, FancyMenu) are blocked with a reason shown in the mod manager.

## Change sequence

`mod-jvm-host` → `mod-fabric-boot` → `mod-net-bridge` → `mod-render-backend` → `mod-frame-compose` →
`mod-input-screens` → `mod-entity-render-hooks` → `mod-sound-bridge` → `mod-manager-ui`.
Spikes first: SPIR-V combined samplers through naga, SDL3 offscreen video driver off the main thread, and the
`EmbeddedChannel` behaviour of `Connection.setupInboundProtocol`.
