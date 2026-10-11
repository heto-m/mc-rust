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
