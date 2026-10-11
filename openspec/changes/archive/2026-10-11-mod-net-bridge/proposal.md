# Proposal

## Why

Mods only work if Java's client world follows the real server. Rust owns the socket and protocol state, so
Java's vanilla `Connection`, its listeners and Fabric's networking mixins must run on a copy of our stream, and
mod packets must go out through our connection.

## What Changes

- A clientbound tap in Rust: every decoded frame `(id, payload)` is copied on the reader thread into a Java ring,
  tagged with the phase (login, configuration or play) that Rust tracks from the clientbound transition packets.
- Java side: a netty `EmbeddedChannel` (`BridgeChannel`) carrying a vanilla `Connection(PacketFlow.CLIENTBOUND)`
  pipeline. Vanilla codecs are kept and only the framing handlers are pass-throughs. Frames are fed on the
  Minecraft thread at the start of each frame, so packets are handled inline.
- `Connection.connect` is redirected so `ConnectScreen` runs normally against `BridgeChannel` and asks Rust to
  connect. A Rust-initiated join opens `ConnectScreen` in Java the same way.
- An outbound filter: Java's serverbound frames, tagged with Java's outbound phase, are forwarded only when the
  phases match and the packet class is on the forward list. That list covers custom payloads, custom query
  answers, container and screen actions, and chat/commands, which go through Rust's signer. Everything Rust owns
  (acknowledgements, keep-alive, movement, known packs, brand) is dropped.
- Player shadow: Rust's local player state overwrites Java's `LocalPlayer` every tick, and vanilla movement and
  movement packets are suppressed in Java.
- Disconnects propagate both ways.
- Measurement of world duplication: Java heap and CPU per loaded chunk column.

Out of scope:
- Modded-server content registries in Rust (mod blocks and entities unknown to Rust).
- Java's answer to `SelectKnownPacks`.
- Rendering of anything Java receives.
- NeoForge.
- Backing Java chunk storage with Rust's mirror (documented as a follow-up).

Acceptance test: `mcr --mods` with Fabric API joins a local vanilla 26.3 server (offline). Java's
`ClientLevel` holds the same loaded chunk set and block states at 20 sampled positions as Rust's world, and the
same entity ids. A test mod's `ClientPlayNetworking` payload reaches the server, which the server log shows as an
unknown custom payload. The server sees exactly one copy of each keep-alive and movement packet.

## Capabilities

### New Capabilities
- `mod-net-bridge`: clientbound packet tap, Java embedded connection, outbound filter, player shadow, disconnect
  propagation.

### Modified Capabilities

## Impact

- `mcr-net` (frame tap), `mcr-modbridge` (phase tracker, rings, filter), `mcr-client` (chat API), `java/mcr-bridge`.
