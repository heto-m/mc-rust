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
  `McrBoot.main(long table, String[] args)`. Flags: `-Djava.class.path=<boot jar + loader + libs>` (Knot loads `client.jar` from `fabric.gameJarPath`),
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
- **Warm-up (AOT cache).** Java starts from a JDK 25 AOT cache per mod set (`mcr_jvm::aot`), stored in
  `<game dir>/cache/jvm-aot/<key>.aot`. The key hashes the JDK, the JVM options other than `-D`, the class path
  (size and modification time) and every mod jar's content, plus `BRIDGE_VERSION`. The first session of a mod set
  records (`-XX:AOTMode=record`; about 1 s slower to the title, and 4–5 s more at quit while `DestroyJavaVM`
  writes the recording). After a clean quit mcr starts itself as a detached helper (`mcr --jvm-aot-create <dir>
  <key>`, nice 10, 3–4 s for 29 mods) that runs `java -XX:AOTMode=create` and renames the result into place.
  A session that starts while the helper runs, or before a recording is built, runs without the cache. HotSpot
  validates the cache and ignores a mismatched one. On Linux mcr also checks `/proc/self/maps` and deletes a
  cache the JVM refused, so the next session records again. Two caches are kept (least recently used goes), a
  cache over 1 GiB is dropped, and a key that failed three times stops recording. `MCR_JVM_AOT=off` disables
  it. The cache holds the classes of all loaders, Knot's included, in their final, mixin-transformed form. A
  class is taken from it only when its bytes match, so mixins still run but parsing and verification are
  skipped. With ZGC, JDK 25 neither links classes ahead of time nor caches heap objects. Lambdas and
  other hidden classes from Knot, signed jars (Fabric Loader) and pre-Java-6 class files are not cached.
  With 29 mods the cache saves about 0.8 s of 11.1 s to the title screen (1.1 s with a cold page cache) and
  does not change the frame rate after joining (numbers in the `mod-jvm-warmup` design).

## Seams (vanilla 26.3 classes)

| Seam | Hook | Owner |
|---|---|---|
| GPU | `PreferredGraphicsApi.getBackendsToTry` → `McrGpuBackend` implementing `com.mojang.renderpearl.api.device.GpuBackend`; `createDevice` returns `new FrontendGpuDevice(new McrDeviceBackend())` (`GpuDeviceBackend`, `CommandEncoderBackend`, `RenderPassBackend`, `GpuSurfaceBackend`) | Java records, Rust submits |
| Shaders | Frontend `GlslCompiler` already emits SPIR-V (shaderc). The backend gets `SpvModule` plus flattened bindings (set 0, binding = uniform index) → Rust splits combined image samplers → naga `spv-in` → wgpu | Rust |
| Window | SDL3 `offscreen` video driver (hint set in `RenderSystem.initBackendSystem`), so `Window` works on a fake window. Size and scale are pushed per frame | Rust |
| Input | `ModInput` events ride the control ring as `MSG_INPUT` ahead of each `MSG_FRAME`; `McrInput.deliver` calls `KeyboardHandler.keyPress/textInput/textEditing` and `MouseHandler.onMove/onButton/onScroll/onDrop` with SDL3 values. `InputConstants.isKeyDown/grabMouse/releaseMouse`, `Window.isFocused/selectCursor`, `ClipboardManager`, `TextInputManager`, `SDL_GetModState` sites are redirected. Key and button events also pass through `SDLEventHandler.handleKeyEvent/handleMouseButtonEvent` with vanilla's body skipped, so mods hooked there (malilib's hotkeys) see them and can consume them | Rust |
| Network | `Connection.connect` is redirected to a `BridgeChannel` (netty `EmbeddedChannel` confined to the render thread by its own event loop) carrying vanilla codecs, so `ConnectScreen` runs unchanged while Rust connects. Frames travel in two net rings (32 MiB in, 4 MiB out) tagged with phase and session; outbound packets are filtered by (phase, packet class), chat goes to Rust as text | Rust owns socket and state |
| Player | Rust's local player and camera overwrite `LocalPlayer`/`Camera` every tick and frame, unless a mod moved the camera (see camera handoff). Java's camera type leads the perspective (see perspective). Vanilla gameplay keybinds except F5 are suppressed in Java. Held movement keys reach `ClientInput.keyPresses` only for the tutorial and mods (`PLAYER_INPUT`). What vanilla predicts without a server echo (drops, swings, mining progress, attack-strength resets) arrives as `NET_ACTION` and is replayed with vanilla's own `LocalPlayer` calls | Rust |
| World | Java keeps its own `ClientLevel`/`ClientChunkCache`. Sections compile meshless (`RenderSection.compileSync/compileAsync` → `BridgeSectionMesh`, block entities only); terrain uploads and draw groups, sky, weather and world border are off (weather extraction only proves no mod replaced it). `Camera.alignWithEntity` takes Rust's eye and rotation; the level projection is Rust's | duplicated, see below |
| Sound | `SoundEngine` logic kept. `Library.init`/`acquireChannel` and `Channel`/`Listener` methods forward to `mcr-audio` | Rust plays |

## Frame composition

In mod mode mcr renders into an offscreen `Rgba8Unorm` + `Depth32Float` scene, which is vanilla's `MainTarget`
format and gamma-space look, and blits it to the swapchain at the end. Java's frames are paced by mcr's
(`compose.rs`, `frame.rs`). In lockstep, each frame is posted at the start of an mcr frame and composed in that
frame. In ahead mode, Java renders frame N+1 while mcr composes and presents frame N (see *Pipelining* below).

1. **Post.** The render thread posts `FrameBegin` with Rust's ticks, partial tick, size and camera. Java runs
   exactly those ticks through a `DeltaTracker.Timer` override, applies the size to `Window`, and renders.
2. **Queue.** Java's submits are tagged with a slot and queued rather than submitted: `IMMEDIATE`;
   `WORLD_SOLID` from `LevelRenderer.render`, `WORLD_TRANSLUCENT` after `executeSolid`, `WORLD_AFTER` after the
   translucent features, `POST_WORLD` from `render3dHud` (hand, entity outline blit, post effects); `GUI` around
   `GuiRenderer.render`. `E_SPLIT` cuts the encoder where mcr's submit point changes. Solid and translucent
   share one command buffer, and so do late features and `POST_WORLD`, unless a pass is open. A render pass open
   across a cut continues (`PASS_CONTINUES`/`PASS_RESUMES` in the pass descriptor restore pipeline, bindings and
   scissor).
3. **World.** `mcr-render` ends its world pass after opaque terrain (`ExternalRenderer::after_opaque`). The render
   thread waits for Java's frame (bounded) and submits `IMMEDIATE`, `WORLD_SOLID` and `WORLD_TRANSLUCENT` there;
   then mcr draws translucent terrain and its world effects. In world slots Java's main color and depth are mcr's
   scene color and reverse-Z depth, and Java's projection is mcr's with Y flipped, so world draws land top-down on
   mcr's pixels and depth-test against its terrain; pipelines that read `Projection` get a counter-clockwise
   variant there. `POST_WORLD` gets a private depth, so the hand's depth clear leaves the scene depth alone.
4. **Compose.** At `after_world` it submits `WORLD_AFTER` and `POST_WORLD`; at `before_ui`, after mcr's post
   effects, it submits `GUI` straight into the scene. Projected GUI pipelines get a vertex variant without naga's Y
   flip and scissors and clears are mirrored, so the GUI lands top-down; the menu blur samples the scene through
   fullscreen passes, which need no flip. Before Java reads its main target (copies, screenshots), the scene is
   flipped into Java's real `MainTarget` color once, in renderpearl's bottom-up rows; screenshots of the main target
   wait until the GUI slot ran. If the wait at `after_opaque` timed out, it does not wait again.
5. **Native overlays.** These draw after, then the frame is presented.

**One submit per frame.** "Submits" above means "appends to the frame batch" (`mcr_render::FrameBatch`): mcr's
command buffers and Java's queued ones keep their order, and the frame ends with a single `queue.submit`. mcr's
own encoder stays open across a hook that adds nothing, so the title screen records mcr's frame in one command
buffer. Java's staged bytes still go out with `queue.write_buffer`, which lands before the whole batch, so the
upload ring is reused only per Java submit, and a queued item with writes from another Java submit (or another
encoder) first sends the batch. The late-camera rewrite is a 64-byte copy command buffer in the batch. A Java
command buffer whose recording failed is dropped at `finish`; if a batch is still rejected, Java's work goes back
to its own submits for the session. Readback maps, fence progress and `on_submitted_work_done` are armed in
`Slots::submitted`, after the batch reached the queue.

**Clears.** A full clear of a single-layer texture is not recorded at once. The next pass that renders to it
clears as it loads, a later clear of the same texture replaces it, and ops that read or write the texture
otherwise (copies, readbacks, region clears, a pass sampling it) record it first. A clear that nothing reads
before the next clear of the same texture therefore costs nothing; on the title screen that removes the two
depth-clear passes Java records every frame.

Clears of the main target's color, and of the scene depth in world slots, are dropped. A Java frame that misses
the 2 s deadline detaches the bridge from rendering: mcr's native HUD and entities return and a toast says so.
Java keeps getting frames one at a time; after 30 frames in a row that each finish within 250 ms it is composed
again (REI and JEI reloading their plugins on join stall Java for over 2 s). Three such recoveries per session;
the next stall, Java stopping or Java losing its connection detach for good.

**Pipelining.** The automatic mode runs Java one frame ahead when mcr's frames are uncapped (no vsync and no fps
cap), or when Java's mean frame takes over 8 ms. It returns to lockstep under a cap once Java's mean frame falls
below 4 ms. `MCR_MOD_PIPELINE=lockstep|ahead|auto` overrides the mode. Ahead mode posts the next frame as soon as
Java finishes one. That happens at `before_frame` when Java is faster than mcr, and at `after_opaque` after the
bounded wait when it is slower. One frame stays outstanding at most. Queued work carries its frame index, and
drains stop at the composed frame, so the frame Java records ahead waits in FIFO order. A frame composed after
the mcr frame that posted it saw the previous camera. Java reports its level projection slice, camera and
inverse view rotation (`D_LEVEL_VIEW`), and mcr rewrites that projection to `flip · P · V · T(Cj − C) · R⁻¹` for
its current camera, just before the frame's first world-slot submit. Entities therefore stay on the terrain
while the camera turns. Java's GUI reacts to input one frame later, which is under a millisecond at uncapped
rates. A frame that is late in lockstep is composed in the next mcr frame the same way, instead of being dropped.

**Profiling.** `MCR_MOD_PROFILE=1` prints a line once a second. It shows fps, the render thread's wait for Java
and its submits of Java's work, and Java's frame phases (pre, tick, extract, world, GUI and the rest, published
in `FSTATE_PROF`). It also shows the Rust time in Java's GPU downcalls, and passes, draws, submits and uploads
per Java frame, and that Rust time split by kind: each encoder op, pass replay, command-buffer `finish`,
staging, and the render thread's queue writes and batch bookkeeping (`gpu::backend::cost`). The render thread's
submit time includes finishing mcr's held encoder when a hook adds Java work after it. Java samples its render
thread with JFR (`McrProfiler`) and logs each mod's share every 10
seconds. `MCR_MOD_PROFILE=frame` skips the sampler, which slows Java down. `mcr --screenshot out.png --bench N`, alone or with `--mods` and `--connect … --after-ticks T`, runs N
unpaced frames offscreen and prints the totals. `--bench-timeline S` instead runs unpaced frames until S seconds
after joining (or after Java's title screen appears) and prints the frame rate of every second; with `--mods`
the shell flow also prints when Java's title screen appeared.

Java's camera FOV over vanilla's (zoom mods, FOV effects mods add) reaches Rust as `FSTATE_FOV_SCALE`, clamped to
0.02–4, and scales mcr's projection and mouse sensitivity while Java draws the world. The other way, `FrameBegin`
carries mcr's render distance (the one its fog uses) and entity distance scaling: Java's
`Options.getEffectiveRenderDistance` returns it while Rust paces Java, so Java's fog, entity view scale and culling
match mcr's terrain.

**Camera handoff.** Mods that move Java's camera (Freecam, camera tweaks, `Camera.update` mixins) keep working:
`alignWithEntity` takes Rust's camera only while the camera entity is the player or the server's spectate target,
and then runs vanilla's body with Rust's values swapped into its `setRotation`/`setPosition` calls, so injections
inside it (Camera Overhaul, Freelook, Shoulder Surfing) still apply on top. In third person `setPosition` gets
Rust's eye and the pull-back `move` gets Rust's camera distance (`FRAME_CAMERA_DISTANCE`), so vanilla lands on
Rust's camera and a mod that redirects the pull-back (Shoulder Surfing) offsets from the eye; other `move` calls
are zeroed, since Rust's camera already has them.
At `GameRenderer.extractCamera` Java compares its final camera with Rust's and, when a mod changed it, reports it
in the frame state (`FSTATE_CAMERA*`, with whether it shows the player and whether the mod took the player's
input). The next frame mcr renders terrain from it (bounded to the loaded world, non-finite values ignored), sends
it back as the render camera with `FRAME_CAMERA_HANDOFF`, and Java places its own camera there, so entities and
terrain share one camera with one frame of latency. A roll of the camera's rotation about its view axis counts as
a change too: Java reports it relative to the camera's yaw and pitch (`FSTATE_CAMERA_ROLL`, robust at a vertical
pitch), mcr multiplies it into its view on top of its own damage tilt, and Java re-applies the roll it reported the
frame before when it places its camera. When the mod took the player's input (`player.input` is no
longer a `KeyboardInput`), mcr's player stops moving and turning, Java's movement keys follow the keyboard and its
`turnPlayer` runs, so the mod's camera entity flies. Without a mod the reported flags are 0 and nothing changes.
`turnPlayer` always runs up to `LocalPlayer.turn`; a probe in `Entity.turn` stops it before it rotates the
player (mcr owns the look). If a mod cancelled the turn before that point (Freelook turns only its camera), Java sets `FSTATE_TURN`
and mcr's player stops turning with the mouse while Java draws the world.

**Perspective.** Java's `Options` camera type decides first person, third person back or front while Java draws
the world. Java publishes it at the end of each frame (`FSTATE_PERSPECTIVE`, from `isFirstPerson`/`isMirrored`, so
a mod perspective on top of a vanilla camera type, such as Shoulder Surfing's shoulder view on third person back,
maps to that type), and mcr sets its perspective from it before it builds the frame's player camera. mcr's native
player model, hand and spyglass overlay follow it. F5 is then Java's: `GameplayKeys` leaves `keyTogglePerspective`
alone while mcr's camera paces Java, so vanilla's handler, or a mod that consumes the key first, cycles it, and mcr
ignores the key. Java never writes its camera type to match mcr. On the frame Java switches, its camera type and
`FRAME_DETACHED` disagree, so the alignment runs vanilla's own body (pull-back against Java's level, mirror), the
handoff reports that camera and Java draws the frame from mcr's camera with mcr's detached state (no player model
seen from the eye); mcr renders the next frame from the reported camera while already following the new type, and
the handoff ends one frame later. Without mods, or while the bridge is detached, mcr cycles its own perspective on
F5; after a stall the perspective Java shows wins again once Java is composed.

**Pose roll.** Mods may roll the level's bob pose instead of the camera (Camera Overhaul rotates it at `bobHurt`
HEAD). mcr replaces Java's level projection with its own view, so Java takes the pose `renderLevel` multiplies into
its projection, rebuilds the pose vanilla's `bobHurt` and `bobView` alone would make from the same render state
(`McrViewRoll`), and reports the twist of `pose · vanilla⁻¹` about the view axis (`FSTATE_VIEW_ROLL`, 0 below
1e-5 rad). mcr multiplies it in front of its own death and hurt tilt, so the world rolls and Java's entities roll
with it; the hand already rolls in Java. Pitch, yaw and translation a mod adds to the bob pose are not carried, and
neither is vanilla's view bobbing, which mcr does not draw in the world.

**Weather replacement.** Vanilla's `WeatherEffectRenderer.extractRenderState` runs up to its first statement and
returns. If the level reached its weather step but that statement never ran (a mod cancelled, redirected or
overwrote it, as Particle Rain does), Java sets `FSTATE_WEATHER` and mcr hides its rain and snow columns, keeping
the rain's sky and fog darkening.

**Footsteps.** Once a second Java asks vanilla's `Entity.walkingStepSound` for a step of its shadow player. If
that never reaches `Player.playStepSound` (a mod such as Presence Footsteps cancelled it to play its own), Java
sets `FSTATE_STEPS` and mcr stops playing the player's footsteps. The probe itself never plays a sound.

**Movement in screens.** While a Java screen is open vanilla wipes the player's movement input. Java records the
movement keys a mod let through just before that wipe (`ISTATE_SCREEN_MOVES`, the `MOVE_*` bits), and mcr moves
its player with them, so InvMove-style mods walk with the inventory open. Without such a mod the bits are 0.

**Section visibility.** Java's own visibility graph sees through every bridge section (they have no geometry), so
mcr shares the sections its graph reached from this frame's camera (`SHARED_SECTIONS`: a box in section
coordinates and one bit per section, at most 2^23 bits, republished when the search reruns; `FRAME_SECTIONS` says
it is current). Java skips the block entities of unreached sections and entities whose bounds touch only unreached
sections (leashed entities, vehicles, the player and the camera entity are always kept). Anything outside the box
counts as reached.

In mod mode Java draws entities, block entities, nametags, the first-person hand, glowing outlines, post effects,
particles of non-`minecraft` types, GUI and HUD. Rust draws terrain, sky, clouds, weather, world border, water,
block outline, destroy overlay and vanilla particles. Java drops a `minecraft:*` particle only inside a path mcr
mirrors (`McrParticles`: the packet drain except custom payloads, per-entity, block entity, level, ambient block,
game renderer and particle ticks); vanilla particles that mod code spawns elsewhere stay in Java. Java culls with a finite perspective built from Rust's field
of view (Rust's reverse-Z infinite matrix sends vanilla's frustum walk into an endless loop).

## Input and UI authority

`mcr-app/src/shell/route.rs` decides per frame who gets input, from the state Java publishes in the shared
input block (screen, overlay, grab, text input, cursor, GUI scale, quit):

| Authority | When | Input goes to |
|---|---|---|
| `Native` | no Java GUI (no mods, booting, detached) | mcr only |
| `Overlay` | an mcr screen above Java's GUI ("mcr" button, mod manager) | mcr only |
| `JavaScreen` | a Java screen, the title or Java's loading overlay | Java only |
| `JavaGame` | in game, no screen | both; gameplay keys act in mcr, Java's vanilla gameplay mappings stay suppressed (F5 is Java's) |

Java owns inventory, chat, commands, advancements, the player list, F3 and Esc in mod mode. mcr keeps its
container model but opens no native game screens. Java's "mcr" button on the title and pause screens opens the
native menu (options, mod manager). Realms shows a "not supported" toast; Singleplayer opens Java's own world
screens (see *Singleplayer* below). Quitting from Java quits mcr. Held keys are released on every authority change, so nothing sticks across a screen.

Vanilla key mappings live in mcr's `options.txt` only. `MSG_KEYS` sends every vanilla mapping to Java whenever
mcr's settings change (and before Java's first frame), so stale lines in Java's own `options.txt` never win. After
`KeyMapping.resetMapping` or `Options.save` (vanilla's Controls screen, Controlling), Java reports the vanilla
mappings it rebound as `REPORT_KEYS`, and mcr applies and saves them. Mod mappings stay Java-only. Caps Lock and
Num Lock start from the keyboard LEDs on Linux (`/sys/class/leds`).

Key presses carry their text to Java unless an IME composition is in progress; then only IME commits do. winit
reports `Ime::Enabled` on Wayland as soon as the compositor offers text input (KWin always does), even with no input
method running, so the route keys on a non-empty preedit, never on `Enabled`.

## Singleplayer

Java's `SelectWorldScreen` and `CreateWorldScreen` stay, so mods that extend them keep working, but worlds run on
mcr's managed vanilla server and are joined through the net bridge like a multiplayer server. Java's integrated
server never starts.

- **Saves.** `-Dmcr.bridge.saves`/`-Dmcr.bridge.backups` point Java's `LevelStorageSource` (only the one rooted at
  its own `<game dir>/java/saves`) at mcr's `<game dir>/saves`, so both clients list the same worlds.
- **Play and Create.** `WorldOpenFlows.openWorld` (world list, keyboard, quick play) and
  `CreateWorldScreen.createWorldAndCleanup` (after vanilla's confirmations, before the folder is created) are
  cancelled and sent as `REPORT_WORLD` (`play\t<folder>`, or `create` with `key=value` lines). The shell starts the
  session as a *Java flow*: no native screens, and every update goes back as `MSG_WORLD` (`WORLD_STATUS`,
  `WORLD_EULA`, `WORLD_READY`, `WORLD_FAILED`, `WORLD_STOPPED`). Java shows them on `McrWorldScreen` (progress with
  Cancel), a `ConfirmScreen` for the EULA and an `AlertScreen` for failures. World types other than default,
  superflat, large biomes and amplified, and debug mode, are refused on the create screen with a toast; data
  packs, experiments and game rule edits are not applied.
- **No Java writes.** `McrWorlds` tracks `IDLE`, `PREPARING`, `LOCAL` and `STOPPING`. While not idle,
  `LevelStorageSource.createAccess`/`validateAndCreateAccess` throw ("the world is open in mcr") and Play/Create
  only toast. `Minecraft.doWorldLoad` is always cancelled in mod mode (access and stem closed, title screen).
- **Joining.** After `WORLD_READY` Rust connects to the local port; the next `NET_OPEN` opens `ConnectScreen` with
  the world list as parent, so a failed join or any later disconnect lands there.
- **Pause.** Java publishes `Gui.isPausing()` as `JAVA_PAUSING`. In a local world the shell sends `tick freeze`
  while it holds and `tick unfreeze` when it clears. Java's own `pause` follows the same rule (its `runTick` store
  is redirected), so `isPaused()`, sounds and F3+Esc behave as in vanilla singleplayer.
- **Save and Quit.** `PauseScreen` sees `isLocalServer()` as true for its label only; `disconnectFromWorld` takes
  the saving path and then shows `McrWorldScreen` in saving mode. Rust's session end saves and stops the server,
  and `WORLD_STOPPED` sends Java to its title screen. Open to LAN needs an integrated server, so it never appears.
- `--singleplayer <folder>` in mod mode waits for Java's GUI, then runs the same flow (the first status message
  opens the progress screen).

## World duplication

Java's `ClientLevel` parses every chunk again (`ClientChunkCache.replaceWithPacketData` → `LevelChunk`, `PalettedContainer`,
`DataLayer` light). At render distance 12 this costs about 60–100 MB of heap plus the decode CPU on the Java
thread. Accept it for now. Later, back `LevelChunkSection` storage with a `BitStorage` implementation over Rust's
off-heap section arrays (`BitStorage` is an interface), if measurements say it pays.

Bridge sections see through every face, so Java's visible section list is every section in the frustum within the
render distance (tens of thousands at 32). Vanilla walks it twice per frame: the dirty-state walk that schedules
compiles and the block-entity walk. The bridge skips the first while no section turned dirty or moved and the list
was not cleared (`SectionUpdates`), hands the second a cached list of sections that hold block entities
(`BlockEntitySections`), and lets meshless sections compile without neighbour chunks. In a still view at render
distance 32 this halved extraction (0.41 to 0.20 ms per Java frame). Turning past 2 degrees rebuilds the list, as
vanilla does.

Measured with 29 mods at 1920x1080, render distance 32 (Ryzen 7 5800X, RX 6800 XT): mcr's own frame about 0.6 ms,
Java's frame 0.6 to 1.0 ms with the ahead pipeline, about 1100 to 1200 fps offscreen and in a headless gamescope
window alike. BetterF3's overlay nearly doubles Java's frame (1.5 to 1.8 ms; gui 0.09 to 0.42, extract up 0.35,
four times the upload), so read frame rates with F3 closed or from `MCR_MOD_PROFILE=frame`. ZGC with the default
2048 MiB heap keeps a 0.7 to 0.9 GB live set, pauses under 0.1 ms and has no allocation stalls; 1536 MiB doubles the
collection rate and leaves little headroom at join, more than 3 GiB buys nothing.

## Policies

- Mod mode is opt-in (mods folder non-empty and enabled). Without mods, no JVM starts.
- Java resolves no remote host names (`jdk.net.hosts.file` with only localhost, `ServerNameResolver` returns
  unresolved addresses), and `Connection.connect` outside `ConnectScreen` throws. Rust owns DNS and sockets. Java's
  HTTP (skins, mod web APIs) goes through a Rust loopback proxy (`proxy.rs`: `CONNECT` for https, absolute-form
  requests for http, forced `Connection: close`) set as `http(s).proxyHost/proxyPort` and as Minecraft's own
  `Proxy`. It reaches only Mojang's hosts plus the policy's `http_hosts` (`*.example.com` for subdomains), only
  global addresses, ports 80 and 443, at most 16 connections, 8 KiB of request head, 30 s idle. A log4j filter
  (`QuietRefusals`) turns the errors mods log for refused hosts (Mod Menu and IPN update checks) into one line per
  logger.
  `System.exit` or a crash in Java freezes or parks the VM for good: the bridge detaches, and mcr exits the
  process on shutdown instead of destroying the VM. A detached net bridge copies and forwards nothing; the native
  client keeps playing.
- With mods, Java has GUI authority: screens, HUD and chat come from Java. Native screens remain for mcr-only UI
  (mod manager, debug, crash fallback). On a Java crash, mcr detaches the bridge and continues natively.
- Rust answers every vanilla protocol acknowledgement. From Java it forwards only custom payloads, custom query
  answers, container and screen actions (including dialogs, advancement tabs and operator block editors), and
  chat/commands (re-signed by Rust). A Java packet-handling failure detaches Java; Rust stays connected.
- Server resource packs: Java's push and pop handlers are cancelled while bridged, and only mcr prompts, downloads
  and answers. After mcr's reload succeeds, `MSG_PACKS` gives Java the applied pack files, which it loads with
  `DownloadedPackSource.pushLocalPack` (and pops when they leave the list), so Java's entities and GUI use them too.
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
