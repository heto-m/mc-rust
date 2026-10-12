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
  hooks so `System.exit` from a mod becomes an orderly mcr shutdown. `Bridge::start` makes every path absolute,
  passes `-Duser.dir=<game dir>` and moves the process into the game dir: mods open `config/…` through
  `java.io`, which resolves relative paths against the process directory and ignores `user.dir`.
- **Java→Rust** calls are FFM downcalls to function pointers Rust passes in a versioned `#[repr(C)]` table
  (no symbol export needed on any OS). **Rust→Java** upcall stubs are registered by Java at boot but are only ever
  invoked on the Java thread inside a downcall. Every other Rust→Java flow is a message in an SPSC ring that
  Java drains, with a futex/condvar doorbell. Bridge entries live in a second, `mcr-modbridge`-owned table
  (`ExtensionTable`, magic `MCRX`, its own version) passed as `-Dmcr.bridge.table`. `mcr-boot.jar` is a
  Fabric system library, so Knot shares the one `dev.mcr.boot` copy. Rust never looks up Minecraft or mod
  classes by name; it reaches Knot-loaded code only through rings and Java-registered upcall stubs. Bulk data (packets, draw streams, PCM) lives in Rust-owned
  off-heap memory viewed from Java as `MemorySegment`s, so nothing is copied per call.
- **Measured call costs** (`mod-jvm-host`, Ryzen 7 5800X): FFM downcall 3.2 ns critical and 3.9 ns normal,
  upcall from Rust 20 ns, and a JNI static call from Rust 55 ns. The cross-thread doorbell handoff costs more
  than any call, so rings and batching stay the shape.
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
| Input | `ModInput` events ride the control ring as `MSG_INPUT` ahead of each `MSG_FRAME`; `McrInput.deliver` calls `KeyboardHandler.keyPress/textInput/textEditing` and `MouseHandler.onMove/onButton/onScroll/onDrop` with SDL3 values. `InputConstants.isKeyDown/grabMouse/releaseMouse`, `Window.isFocused/selectCursor`, `ClipboardManager`, `TextInputManager`, `SDL_GetModState` sites and `turnPlayer` are redirected | Rust |
| Network | `Connection.connect` is redirected to a `BridgeChannel` (netty `EmbeddedChannel` confined to the render thread by its own event loop) carrying vanilla codecs, so `ConnectScreen` runs unchanged while Rust connects. Frames travel in two net rings (32 MiB in, 4 MiB out) tagged with phase and session; outbound packets are filtered by (phase, packet class), chat goes to Rust as text | Rust owns socket and state |
| Player | Rust's local player and camera overwrite `LocalPlayer`/`Camera` every tick and frame. Vanilla gameplay keybinds are suppressed in Java | Rust |
| World | Java keeps its own `ClientLevel`/`ClientChunkCache`. Sections compile meshless (`RenderSection.compileSync/compileAsync` → `BridgeSectionMesh`, block entities only); terrain uploads and draw groups, sky, weather and world border are off. `Camera.alignWithEntity` takes Rust's eye and rotation; the level projection is Rust's | duplicated, see below |
| Sound | `SoundEngine` logic kept. `Library.init`/`acquireChannel` and `Channel`/`Listener` methods forward to `mcr-audio` | Rust plays |

## Frame composition

In mod mode mcr renders into an offscreen `Rgba8Unorm` + `Depth32Float` scene, which is vanilla's `MainTarget`
format and gamma-space look, and blits it to the swapchain at the end. Java's frames run in lockstep with
mcr's (`compose.rs`, `frame.rs`).

1. **Post.** The render thread posts `FrameBegin` with Rust's ticks, partial tick, size and camera. Java runs
   exactly those ticks through a `DeltaTracker.Timer` override, applies the size to `Window`, and renders.
2. **Queue.** Java's submits are tagged with a slot and queued rather than submitted: `IMMEDIATE`;
   `WORLD_SOLID` from `LevelRenderer.render`, `WORLD_TRANSLUCENT` after `executeSolid`, `WORLD_AFTER` after the
   translucent features, `POST_WORLD` from `render3dHud` (hand, entity outline blit, post effects); `GUI` around
   `GuiRenderer.render`. `E_SPLIT` cuts the encoder at each boundary; a render pass open across a cut continues
   (`PASS_CONTINUES`/`PASS_RESUMES` in the pass descriptor restore pipeline, bindings and scissor).
3. **World.** `mcr-render` ends its world pass after opaque terrain (`ExternalRenderer::after_opaque`). The render
   thread waits for Java's frame (bounded) and submits `IMMEDIATE`, `WORLD_SOLID` and `WORLD_TRANSLUCENT` there;
   then mcr draws translucent terrain and its world effects. In world slots Java's main color and depth are mcr's
   scene color and reverse-Z depth, and Java's projection is mcr's with Y flipped, so world draws land top-down on
   mcr's pixels and depth-test against its terrain; pipelines that read `Projection` get a counter-clockwise
   variant there. `POST_WORLD` gets a private depth, so the hand's depth clear leaves the scene depth alone.
4. **Compose.** At `after_world` it submits `WORLD_AFTER` and `POST_WORLD`, copies the scene into Java's real
   `MainTarget` color, flipped because renderpearl targets are bottom-up, submits `GUI` so the menu blur samples
   the world, and copies the result back, flipped. If the wait at `after_opaque` timed out, it does not wait again.
5. **Native overlays.** These draw after, then the frame is presented.

Clears of the main target's color, and of the scene depth in world slots, are dropped. A Java frame that misses
the 2 s deadline detaches the bridge from rendering: mcr's native HUD and entities return and a toast says so.
Java keeps getting frames one at a time; after 30 frames in a row that each finish within 250 ms it is composed
again (REI and JEI reloading their plugins on join stall Java for over 2 s). Three such recoveries per session;
the next stall, Java stopping or Java losing its connection detach for good.

Java's camera FOV over vanilla's (zoom mods, FOV effects mods add) reaches Rust as `FSTATE_FOV_SCALE`, clamped to
0.02–4, and scales mcr's projection and mouse sensitivity while Java draws the world.

In mod mode Java draws entities, block entities, nametags, the first-person hand, glowing outlines, post effects,
particles of non-`minecraft` types, GUI and HUD. Rust draws terrain, sky, clouds, weather, world border, water,
block outline, destroy overlay and vanilla particles. Java culls with a finite perspective built from Rust's field
of view (Rust's reverse-Z infinite matrix sends vanilla's frustum walk into an endless loop).

## Input and UI authority

`mcr-app/src/shell/route.rs` decides per frame who gets input, from the state Java publishes in the shared
input block (screen, overlay, grab, text input, cursor, GUI scale, quit):

| Authority | When | Input goes to |
|---|---|---|
| `Native` | no Java GUI (no mods, booting, detached) | mcr only |
| `Overlay` | an mcr screen above Java's GUI ("mcr" button, mod manager) | mcr only |
| `JavaScreen` | a Java screen, the title or Java's loading overlay | Java only |
| `JavaGame` | in game, no screen | both; gameplay keys act in mcr, Java's vanilla gameplay mappings stay suppressed |

Java owns inventory, chat, commands, advancements, the player list, F3 and Esc in mod mode. mcr keeps its
container model but opens no native game screens. Java's "mcr" button on the title and pause screens opens the
native menu (options, mod manager). Singleplayer and Realms show a "not supported" toast. Quitting from Java
quits mcr. Held keys are released on every authority change, so nothing sticks across a screen.

## World duplication

Java's `ClientLevel` parses every chunk again (`ClientChunkCache.replaceWithPacketData` → `LevelChunk`, `PalettedContainer`,
`DataLayer` light). At render distance 12 this costs about 60–100 MB of heap plus the decode CPU on the Java
thread. Accept it for now. Later, back `LevelChunkSection` storage with a `BitStorage` implementation over Rust's
off-heap section arrays (`BitStorage` is an interface), if measurements say it pays.

## Policies

- Mod mode is opt-in (mods folder non-empty and enabled). Without mods, no JVM starts.
- Java resolves no remote host names (`jdk.net.hosts.file` with only localhost, `ServerNameResolver` returns
  unresolved addresses), and `Connection.connect` outside `ConnectScreen` throws. Rust owns DNS and sockets. Java's
  HTTP (skins, mod web APIs) goes through a Rust loopback proxy (`proxy.rs`: `CONNECT` for https, absolute-form
  requests for http, forced `Connection: close`) set as `http(s).proxyHost/proxyPort` and as Minecraft's own
  `Proxy`. It reaches only Mojang's hosts plus the policy's `http_hosts` (`*.example.com` for subdomains), only
  global addresses, ports 80 and 443, at most 16 connections, 8 KiB of request head, 30 s idle.
  `System.exit` or a crash in Java freezes or parks the VM for good: the bridge detaches, and mcr exits the
  process on shutdown instead of destroying the VM. A detached net bridge copies and forwards nothing; the native
  client keeps playing.
- With mods, Java has GUI authority: screens, HUD and chat come from Java. Native screens remain for mcr-only UI
  (mod manager, debug, crash fallback). On a Java crash, mcr detaches the bridge and continues natively.
- Rust answers every vanilla protocol acknowledgement. From Java it forwards only custom payloads, custom query
  answers, container and screen actions (including dialogs, advancement tabs and operator block editors), and
  chat/commands (re-signed by Rust). A Java packet-handling failure detaches Java; Rust stays connected.
- Performance and renderer mods that need OpenGL (Sodium, Iris, ImmediatelyFast, Distant Horizons, Xaero's,
  JourneyMap, FancyMenu) are blocked with a reason shown in the mod manager. Results for real mods are in
  [mod-compat.md](mod-compat.md).
- The resolver treats `minecraft`, `fabricloader`, `java`, the bridge mod and `mixinextras` (bundled with the
  pinned loader) as provided. Manifests are parsed as leniently as Fabric's: raw control characters in strings
  are accepted.

## Change sequence

`mod-jvm-host` → `mod-fabric-boot` → `mod-net-bridge` → `mod-render-backend` → `mod-frame-compose` →
`mod-input-screens` → `mod-entity-render-hooks` → `mod-sound-bridge` → `mod-manager-ui`.
Spikes first: SPIR-V combined samplers through naga, SDL3 offscreen video driver off the main thread, and the
`EmbeddedChannel` behaviour of `Connection.setupInboundProtocol`.
