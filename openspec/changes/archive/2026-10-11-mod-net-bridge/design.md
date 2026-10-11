## Context

Vanilla 26.3 networking (all under `net.minecraft`):

- `network.Connection` extends `SimpleChannelInboundHandler<Packet<?>>`.
  - `configureSerialization(pipeline, inbound, local, monitor)` adds the handlers `splitter`, flow control,
    `inbound_config` (client side), `prepender` and `outbound_config`.
  - `setupInboundProtocol(ProtocolInfo, listener)` and `setupOutboundProtocol(ProtocolInfo)` swap codecs through
    `UnconfiguredPipelineHandler` tasks written on the channel, and add the `bundler`/`unbundler` handlers.
  - Compression and encryption handlers are added only by `setupCompression`/`setEncryptionKey`.
  - Dispatch is `channelRead0` → `genericsFtw` → `packet.handle(listener)`. Play handlers then call
    `PacketUtils.ensureRunningOnSameThread(packet, listener, minecraft.packetProcessor())`, which queues work
    when called off the game thread and runs it inline when called on it.
  - `PacketProcessor.processQueuedPackets()` runs once per frame in `Minecraft.runTick`.
- Connection setup: `ConnectScreen.connect` calls `Connection.connect(InetSocketAddress, EventLoopGroupHolder,
  Connection)`, then `initiateServerboundPlayConnection(…)` (which sends `ClientIntentionPacket`), then
  `ServerboundHelloPacket`.
- Transitions:
  - `ClientHandshakePacketListenerImpl.handleLoginFinished` switches to configuration.
  - `ClientConfigurationPacketListenerImpl.handleConfigurationFinished` switches to play.
  - `ClientPacketListener.handleConfigurationStart` switches back to configuration.
- No memory mode passes `Packet` objects unencoded. Local connections still encode and only change framing
  (`LocalFrameEncoder`/`LocalFrameDecoder` with `HiddenByteBuf`).

Fabric API's networking module mixes into `Connection`, `PacketEncoder`, `PacketDecoder`, `IdDispatchCodec`, the
custom payload/query packets and all four client listeners. Registry sync uses `c:version`/`c:register` both ways
plus `fabric:registry/sync` (server→client) and `fabric:registry/sync/complete` (client→server) during
configuration. A vanilla server never sends them.

## Decisions

**Protocol ownership: Rust.** Rust performs handshake, login (auth, encryption, compression) and every vanilla
acknowledgement. Java is a listener that must stay in the same phase. Rust tracks the phase on the reader thread
from clientbound packets alone:
- login → configuration on `LoginFinished`;
- configuration → play on `FinishConfiguration`;
- play → configuration on `StartConfiguration`.

These are exactly the packets after which vanilla's client switches its inbound decoder. The tracker lives in
`mcr-modbridge` and uses the generated packet ids of `mcr-protocol`'s version layer.

**Clientbound tap.** `mcr-net` gains `ConnectOptions::frame_tap: Option<Box<dyn FnMut(i32, &[u8]) + Send>>`,
called on the reader thread after decompression and decryption, before the frame reaches the game thread. The
bridge tap copies `[phase u8][id varint][payload]` into a 32 MiB SPSC ring. In the login phase it skips `Hello`
(encryption request) and `LoginCompression`, because Java's pipeline has neither stage. The tap never blocks. On a
full ring it spills into a bounded Rust queue (256 MiB). Past that it marks Java desynchronised, Java gets a
disconnect, and Rust keeps playing.

**Java connection.** This replaces `mod-fabric-boot`'s interim `ConnectScreen.connect` cancel.
`BridgeChannel extends io.netty.channel.embedded.EmbeddedChannel`. The bridge redirects
`Connection.connect(InetSocketAddress, EventLoopGroupHolder, Connection)` (static, used by `ConnectScreen`), which:
1. Builds the pipeline with `Connection.configureSerialization(pipeline, CLIENTBOUND, false, null)`.
2. Replaces `splitter` and `prepender` by name with pass-through handlers. The names are kept for mods that look
   them up.
3. Adds `Connection.configurePacketHandler`, registers and activates the channel (`channelActive` sets
   `Connection.channel`), and returns a succeeded `ChannelFuture`.
4. Downcalls `request_connect(host, port)`. Rust ignores it when the join was Rust-initiated.

`ConnectScreen` then proceeds unmodified. `initiateServerboundPlayConnection` sets up the login protocols, and its
`ClientIntentionPacket` and `ServerboundHelloPacket` are dropped by the outbound filter. A Rust-initiated join
posts `Connect{host, port}`, and Java calls `ConnectScreen.startConnecting(…)`.

**Feeding.** In `awaitFrame` (the `runTick` HEAD hook), Java drains the clientbound ring:
- Each frame is wrapped as an unpooled `ByteBuf` over the ring segment (`Unpooled.wrappedBuffer(segment.asByteBuffer())`;
  decoders copy what they keep) and passed to `writeInbound`.
- Packets are decoded and dispatched inline on the Minecraft thread, so `ensureRunningOnSameThread` runs handlers
  immediately.
- Protocol swaps happen before the next frame is decoded.
- `runPendingTasks()` then flushes netty tasks.

Phase check: the Java bridge records the inbound `ProtocolInfo.id()` from a `setupInboundProtocol` TAIL hook. A
frame whose tag differs from Java's phase is a desync: log it and drop the frame.

**Outbound.** `EmbeddedChannel` keeps written `ByteBuf`s in its outbound queue. After each drain and at the end of
`runTick`, Java moves each frame to the serverbound ring as `[phase][bytes]`, where phase comes from a
`setupOutboundProtocol` TAIL hook. On the game thread, Rust parses the id, then forwards the frame only when
Rust's outbound phase equals the tag and the packet is on the forward list. Otherwise it drops it and counts it per
packet class.

| Phase | Forwarded from Java | Dropped (Rust owns) |
|---|---|---|
| login | `ServerboundCustomQueryAnswerPacket` (mod mode: Rust stops answering custom queries itself) | intention, hello, key, login ack, cookie response |
| configuration | `ServerboundCustomPayloadPacket` except `minecraft:brand` | client information, known packs, finish ack, keep-alive, pong, resource pack response, code of conduct |
| play | custom payload (except brand); `ContainerClick`, `ContainerButtonClick`, `ContainerClose`, `ContainerSlotStateChanged`, `RenameItem`, `SelectTrade`, `SetBeacon`, `EditBook`, `SignUpdate`, `PlaceRecipe`, `RecipeBookChangeSettings`, `RecipeBookSeenRecipe`, `SetCreativeModeSlot`, `CommandSuggestion`, `ClientCommand` | keep-alive, pong, movement, player input, teleport accept, chunk batch received, tick end, chat ack/session, player loaded, carried item, use/attack/dig |

Chat and commands never pass as raw frames. A HEAD hook cancels `ClientPacketListener.sendChat(String)`,
`sendCommand(String)` and `sendUnattendedCommand(…)` and downcalls `send_chat(kind, text)`, so Rust's chat session
signs and orders them. The forward list is data in `mcr-modbridge`, which `mod-input-screens` extends as Java
screens take over.

**Player shadow.** Rust owns the local player. Every Rust tick, a `PlayerState` message carries position, previous
position, rotation, velocity, on-ground, pose flags, selected slot and vehicle id. Java applies it at the TAIL of
`LocalPlayer.tick()`, with `setPos`, `xo/yo/zo`, `setYRot/setXRot`, `setDeltaMovement`, `setOnGround`, and pose
and selected slot through accessors. Hooks:
- HEAD of `LocalPlayer.aiStep()` zeroes the movement input.
- `LocalPlayer.sendPosition()` and `sendChanges()` are cancelled, which saves the work and the filter traffic.
- `Minecraft.handleKeybinds()` skips vanilla gameplay mappings (attack, use, pick, drop, swap, hotbar, inventory,
  movement) but still runs mod-registered `KeyMapping`s.

**Disconnect.** When Rust closes, Java receives `Disconnected(reason)` and calls `connection.disconnect(Component)`,
which shows Java's screen. When Java closes (`BridgeChannel.close()` from `Connection.disconnect`), it downcalls
`request_disconnect`.

**World duplication (question 5).** Java parses every chunk again:
- `ClientChunkCache.replaceWithPacketData` → `LevelChunk.replaceWithPacketData` → `LevelChunkSection.read` into a
  `PalettedContainer` per section, plus `DataLayer`s (2 KiB per section per light layer) via `LevelLightEngine`.
- Estimate: 20–80 KiB of blocks and up to about 100 KiB of light per column. At render distance 12 (about 625
  columns), that is 60–100 MB of heap, plus Java-thread decode CPU.
- Java terrain meshing is disabled in `mod-entity-render-hooks`, so the remaining cost is parse plus light.

This change measures it (task 4.2). The follow-up, if the measurement warrants it: `BitStorage` is an interface, so
a `MirrorBitStorage` can view Rust's off-heap section arrays via `MemorySegment`. That requires Rust's section
layout to match `SimpleBitStorage` (values per long, no spanning) and global palette ids to match the registry.
Light could likewise read Rust's arrays. Swapping `ClientChunkCache` itself is harder: `ClientLevel` constructs it
directly, and Lithium-class mods mix into `PalettedContainer`.

## Risks / spikes

- **EmbeddedChannel behaviour** (spike first): `setupInboundProtocol` syncs on `writeAndFlush` of an
  `UnconfiguredPipelineHandler` task. Verify that this completes inline on `EmbeddedEventLoop`, and that
  `ConnectScreen`'s background thread (it calls `Connection.connect` off the main thread) is safe when the
  `EmbeddedChannel` is later driven only from the Minecraft thread. Fallback: a netty `LocalChannel` pair on the
  `EventLoopGroupHolder.local()` group, with frames written by a bridge-owned local server end.
- Container `stateId` agreement between Java screens and Rust's inventory, since both apply the same server
  packets.
- Modded servers with content mods need registries in Rust, which is out of scope.
- Configuration-phase payloads from Java are forwarded only while Rust is still in configuration. Rust acknowledges
  `FinishConfiguration` at once, so a mod's configuration handshake that lags by a Java frame is dropped (counted as
  wrong-phase). Vanilla servers need none. Modded servers will need Rust to hold its finish acknowledgement until
  Java's (Java's dropped `finish_configuration` frame is the signal), with a timeout.

## Spike 0.1: EmbeddedChannel seam (go, no fallback)

`NetSeamSelfTest` (opt-in `-Dmcr.bridge.test=net-seam`, run by `tests/net_seam.rs`) builds a `BridgeChannel` on the
render thread of the booted client, runs vanilla's `initiateServerboundPlayConnection` with a real
`ClientHandshakePacketListenerImpl`, and feeds the captured `LoginFinished` frame:

- `setupInboundProtocol` completes inline: the `UnconfiguredPipelineHandler` task runs inside `writeAndFlush` and the
  future is already done, so `syncUninterruptibly` never waits (first call 57 ms, all class loading).
- One `writeInbound` decodes `LoginFinished` and dispatches it inline. The listener becomes
  `ClientConfigurationPacketListenerImpl` and the inbound phase is configuration before the call returns (first call
  122 ms, again class init).
- Serverbound frames leave tagged `0:intention, 1:login_acknowledged, 2:brand, 2:client_information`, each with the
  phase of the encoder that wrote it.
- **Threading finding:** netty's `EmbeddedEventLoop.inEventLoop()` is always true and its queues are `ArrayDeque`s, so
  a send from another thread (ConnectScreen's connector, a mod's worker) would run the pipeline on that thread.
  `BridgeChannel.eventLoop()` therefore returns a `RenderThreadLoop` that is "in the loop" only on the render thread
  and queues everything else in a `ConcurrentLinkedQueue` that the render thread runs before it feeds packets. The
  spike checks it: a keep-alive sent from another thread is not written until `runTasks()`, then goes out last.
- ConnectScreen's connector thread builds the channel unregistered. The render thread registers (activates) it only
  after the connector's `run()` returns (`ConnectScreen$1` HEAD/RETURN hooks), so `Connection`'s pending actions
  (inbound protocol, intention, hello) run on the render thread too.

The netty `LocalChannel` fallback is not needed.

## Implementation notes and deviations

**Wire format.** Two new Rust-owned rings, separate from the 64 KiB control ring: clientbound 32 MiB and serverbound
4 MiB (`SHARED_NET_IN_RING`/`SHARED_NET_OUT_RING`, `BRIDGE_VERSION` 2). Every message starts with
`[u8 tag][u8 0][u16 0][u32 session]`, followed by the vanilla frame body (`[varint id][payload]`) for frames. The tag
is the phase for frames and the chat kind for chat. Clientbound kinds: `NET_FRAME`, `NET_OPEN{port, host}`,
`NET_CLOSE{reason}`, `NET_PLAYER`, `NET_PROBE`. Serverbound kinds: `OUT_FRAME`, `OUT_CHAT`, `OUT_DISCONNECT`, `OUT_PROBE`.
Rust's tests parse `BridgeAbi.java` against every constant.

**Sessions instead of a `request_connect` downcall.** `NetLink::hooks(host, port)` opens a session (`NET_OPEN`) before
the TCP connect. When Java has no waiting channel, it calls `ConnectScreen.startConnecting`, which is the Rust-initiated
join. When Java's own `ConnectScreen` is waiting with an unbound channel (a Java-initiated join that reported
`REPORT_CONNECT`, now forwarded to the shell), `NET_OPEN` binds that channel. The relay's drop posts `NET_CLOSE`.
Frames of an open session wait in the ring until its channel is active, and any message from another session is
dropped on both sides. No new downcalls were needed: connect requests reuse `REPORT_CONNECT`, and chat and Java's
disconnect are serverbound ring records (not `send_chat`/`request_disconnect` downcalls), so they stay ordered with
frames.

**Bridge upcalls: none.** All Rust→Java traffic is ring messages, including the world probe, so this change adds no
`register_upcalls` bridge entry. The first change with a synchronous Rust→Java call adds it, registered from
Knot-loaded code. `mcr-jvm`'s `DowncallScope` must then be exposed, so that bridge downcalls can host upcalls.

**Feeding.** Each frame is copied into a pooled `ByteBuf` instead of wrapped over ring memory, because a mod codec may
keep a retained slice after the ring slot is reused. The copy costs one memcpy (see the measurements). A throwing
pipeline disconnects only Java's connection.

**Phase tags.** The inbound phase comes from a `setupInboundProtocol` TAIL hook, as designed. The outbound phase comes
from a `PacketEncoder.encode` HEAD hook, so each frame carries the protocol that actually encoded it. The design's
`setupOutboundProtocol` TAIL hook could not tag correctly: a terminal packet swaps the encoder before the frame reaches
the sink. The `prepender` slot holds `FrameSink`, which writes straight to the serverbound ring (a Java-side spill
queue of at most 16 MiB covers a full ring). There is no outbound-queue drain.

**Rust side.**
- `mcr-net`: `ConnectOptions::frame_tap` runs after the inbound hook on the reader thread.
- `mcr-client`: gained the `Relay` trait and `ClientHooks`/`Client::connect_hooked`. The relay is polled in `pump` and
  `await_play`. Its `tick_end` posts `PlayerState` after every play tick. While it is attached, Rust leaves login
  custom queries to Java.
- `mcr-modbridge::net`:
  - The tap and `PhaseTracker` run on the reader thread.
  - `Clientbound` writes through one mutex shared by the reader thread, the game thread and the pacer. The pacer
    flushes the spill every frame.
  - The forward table is data (`FORWARD`).
  - The relay counts every drop per (phase, packet name). `NetStats` exposes the counts.
- A Java crash or VM end calls `NetShared::detach`. From then on the tap copies nothing, the relay forwards nothing,
  the spill is freed and Rust answers custom queries again. The native client keeps working.

**Java side.** `McrNet` holds the per-frame net state on the render thread:
- It runs queued loop tasks, activates a pending channel, drains the clientbound ring and flushes the out-spill at the
  `awaitFrame` HEAD. At the `runTick` TAIL it runs tasks again and flushes the out-spill.
- Chat, commands and unattended commands are cancelled at HEAD and sent as `OUT_CHAT`.
- `LocalPlayer`:
  - `tick` TAIL applies `PlayerShadow` (position, old position, velocity, rotation, on-ground, shift, sprint, swim,
    pose, selected slot). The vehicle id is carried but not applied, because riding comes from server packets.
  - Movement input is zeroed right after `ClientInput.tick()`.
  - `sendChanges` is cancelled. That covers `sendPosition`, `PlayerInput` and the sprint commands.
- `Minecraft.handleKeybinds` HEAD drains and releases vanilla gameplay keys only.
- `ServerNameResolver.resolveAddress` returns an unresolved address, so Java does no DNS or SRV lookups.
- `Connection.connect` outside a ConnectScreen connector (server-list pings) throws, so Java opens no sockets. Pings
  for Java's server list are a follow-up (Rust pings).

**Measurement probe.** `NET_PROBE` asks Java for its loaded chunks in a square, block state ids at up to 1024
positions, entity ids, the last applied `PlayerState` tick with the resulting position, heap use and net timings. It
answers with `OUT_PROBE`. The test-only `PROBE_DISCONNECT` flag makes Java leave the world the way the pause menu's
Disconnect button does.

**App.** `ModHost::net()` gives the shell a `ModNet`. Every shell connection gets the bridge hooks, and Java's join
requests become shell connects. The GUI path was not run here (no native windows): it compiles, and the same
`NetLink` API is exercised by the integration test.

**Test mod.** `java/mcr-testmod` (built by `java-build`; the new `fabric-api-module <name>` deps line extracts nested
Fabric API jars into `.cache/java/deps`) sends `mcr_testmod:hello` on `ClientPlayConnectionEvents.JOIN`. It sends
`/time query day` 40 ticks later, because Fabric's client-command hook runs at `sendCommand` HEAD before ours and
throws until the server's command tree has arrived.

**Acceptance through a proxy.** A vanilla server never logs custom payloads, so `tests/net_bridge.rs` puts a recording
proxy between Rust and the server and checks the server-side view on the wire:
- exactly one `mcr_testmod:hello` in play, and only Rust's brand;
- one keep-alive answer per keep-alive id;
- one `accept_teleportation` per `player_position` and one `chunk_batch_received` per `chunk_batch_finished`;
- one `player_loaded`, one `client_information` and one `select_known_packs`;
- the test mod's command exactly once, never signed.

Then:
- Java's chunk set, 20 block states and entity ids equal Rust's after 30 s.
- Java's player equals Rust's position at the same tick to 1e-6 after a 20+ block walk (as a spectator, so terrain
  cannot block it).
- Java's pause-menu disconnect closes Rust's socket.
- A second session is kicked by the server, and Java shows `DisconnectedScreen`.

**Housekeeping.** `crates/mcr-modbridge/logs/` (Minecraft log output committed by `mod-fabric-boot`) is untracked and
`/crates/*/logs` is ignored.

## Java HTTP decision

Java keeps the localhost-only hosts file. HTTP that Java legitimately needs (skins and capes from
`textures.minecraft.net`, profile lookups, mod web APIs) will go through a Rust-owned loopback HTTP `CONNECT` proxy:
- Java is started with `-Dhttp(s).proxyHost=127.0.0.1` and `-Dhttp(s).proxyPort`, which both `HttpURLConnection` and
  `java.net.http.HttpClient` honour.
- Through a proxy, Java never resolves the target host, so the hosts file stays as it is.
- The proxy enforces an allow-list (Mojang texture and session hosts, plus per-mod hosts from the mod manager's
  policy), and Rust does the DNS and the socket.

A Java-side allow-list was rejected: it would let Java open its own sockets again. The follow-up task is in
`mod-entity-render-hooks` (player skins are the first consumer).

## Measurements (tasks 4.1, 4.2)

Ryzen 7 5800X, Linux 7.2, Temurin 25.0.4.1, `-Xmx1536m` ZGC, Fabric API + Mod Menu + `mcr_testmod`, vanilla 26.3
server (`view-distance=10`, client asked for 12), debug test profile with dependencies at opt-level 3.

**Tap on the reader thread** (criterion `benches/tap.rs`, a Java stand-in draining the 32 MiB ring, against
`FrameDecoder` decoding the same frame zlib-compressed at threshold 256):

| Frame | Decode | Tap | Tap / decode |
|---|---|---|---|
| 64 B (uncompressed) | 52 ns | 39 ns | 75 % |
| 1 KiB | 2.09 us | 50 ns | 2.4 % |
| 16 KiB | 21.7 us | 460 ns | 2.1 % |

The 1 % target is missed. The tap costs a fixed ~40 ns per frame (mutex, header, VarInt), plus a memcpy. In absolute
terms the integration run copied 50 859 frames (20 MiB) in about 45 s. Fixed cost plus copying is about 5 ms of
reader-thread CPU, about 0.01 % of a core. A lock-free single-writer path is possible if this ever shows up in a profile.

**Java side** (three integration runs):

| Metric | Value |
|---|---|
| Loaded columns (Rust = Java) | 473 |
| Java heap after GC, title screen → joined | 310 → 368, 320 → 392, 340 → 388 MiB |
| Heap per column (upper bound: includes all join state) | 104–156 KiB |
| `handleLevelChunkWithLight` on the render thread | 235–448 us per chunk packet (119–233 ms for the whole join) |
| Feed cost (copy + decode + inline handling), all frames | 17.7–28.2 us per frame on average |
| Spill peak | 0 B (Java kept up at 60 Hz) |

**Extra memory:**
- 36 MiB of ring address space, zero-filled and paged in as traffic passes. The clientbound ring is fully resident
  after 32 MiB of traffic.
- A Rust spill of at most 256 MiB (0 B observed).
- A Java scratch segment of 64 KiB and an out-spill of at most 16 MiB.

**Decision on mirror-backed chunk storage:** not needed now. The joined heap delta (48–72 MiB for 473 columns) sits
inside the 60–100 MB estimate, and parse CPU is 120–230 ms per full join. Revisit at render distance 32 or when the
heap is sized from the mod count (`mod-manager-ui`).

## Risks for `mod-render-backend`

- **Feeding shares the Java frame.** The render thread feeds packets inside `awaitFrame`. A join burst (about 500
  chunk packets at about 250 us each, plus the `LevelChunk`/light work) can make individual Java frames long. Frame
  lockstep must not stall Rust's present on a late Java slot, or the drain needs a time budget.
- **Java now has a world.** Terrain compile is still enabled until `mod-entity-render-hooks`. On the null device that
  only costs CPU. With the real backend, Java would upload and draw sections through it. Disable
  `compileSections`/section draws before or with the real device.
- **Render-thread waits can deadlock.** Bridge channels are confined to the render thread. Any render-thread wait on a
  netty future completed from another thread deadlocks. Waits from other threads are fine, because the render thread
  runs their tasks at its next frame.
- **No bridge upcalls yet.** If the backend needs synchronous Rust→Java calls (resource lifetime callbacks, readback
  completion), add the `register_upcalls` bridge entry. Expose `mcr-jvm`'s `DowncallScope` so that bridge downcalls
  can host upcalls.
- **Version skew in the shared cache.** `.cache/java/*.jar` is shared by every worktree through the `.cache` symlink.
  Parallel agents rebuilding `mcr-bridge.jar` overwrite each other. The stale-jar check catches older jars, not
  different ones.
- **Heap headroom.** About 400 MiB is in use joined at a 1536 MiB cap. GPU resource wrappers and staging are added on
  top.
