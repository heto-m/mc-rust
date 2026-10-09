# Proposal

## Why

The client cannot join a server or render the world without decoding the clientbound play packets that create, move and remove entities, load chunks and light, and change blocks, sounds, borders and time. These packets are the bulk of play-state traffic and need exact wire codecs for protocol 777 (26.3).

## What Changes

- Add typed `Encode`/`Decode` packet definitions for the 66 world and entity clientbound play packets (entity spawn/move/teleport/data/equipment/effects/attributes, block and chunk updates, level events, particles, sounds, border, time, ticking, waypoints and debug subscriptions).
- Chunk, biome and light payloads stay borrowed and undecoded; the split is verified against the `mcr-world` payload decoders.
- Add the low-precision velocity vector, movement delta and position-path encodings used by entity movement.
- Debug subscription payloads decode a small set of kinds and keep the remaining kinds as explicit opaque bytes.

Out of scope: player, UI and container packets, serverbound packets, registry-aware resolution of ids, and the shared placeholder types (components, items, particles, sounds, entity data).

## Capabilities

### New Capabilities
- `protocol-play-world`: wire codecs for the clientbound play world and entity packets.

### Modified Capabilities

## Impact

- `crates/mcr-protocol/src/play/clientbound/world.rs`, `entity.rs` and submodules.
- `mcr-world` becomes a dev-dependency of `mcr-protocol` for payload compatibility tests.
