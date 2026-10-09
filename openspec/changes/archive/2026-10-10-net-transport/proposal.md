# Proposal

## Why

The client needs to move raw Minecraft frames to and from a server before any packet definitions exist. A transport layer independent of packet types lets protocol, login and game code build on a tested, fast foundation.

## What Changes

- New crate `mcr-net`: VarInt-length framing, threshold-based zlib compression, AES-128-CFB8 stream encryption, all toggleable mid-stream.
- `Connection`: blocking `TcpStream` with one reader thread and one writer thread, exchanging `(packet_id, payload)` frames with the game thread over channels, with pooled buffers.
- Exact frame-boundary semantics for enabling compression and encryption in both directions.
- Clean disconnect with an error reason; graceful shutdown.
- Server address parsing (`host[:port]`, default 25565) with a small blocking SRV lookup that falls back silently.
- Tests (framing edge cases, compression boundary, CFB8 known answer, loopback with a fake server) and a decode-throughput benchmark.

## Capabilities

### New Capabilities
- `net-transport`: framing, compression, encryption and the threaded connection API.
- `server-address-resolution`: parsing server addresses and SRV redirection.

### Modified Capabilities

## Impact

- New crate `crates/mcr-net` depending on `mcr-buf`.
- New dependencies: `aes`, `cfb8`, `crossbeam-channel`, `libdeflater`; dev: `criterion`, `flate2` (backend comparison).
- Out of scope: packet definitions, handshake/login/auth flow, Mojang session calls, async I/O, TCP fallback for DNS.
