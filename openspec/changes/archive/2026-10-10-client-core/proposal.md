## Why

Milestone 1 needs a client that joins a vanilla server and keeps full game state, independent of any renderer. Packet codecs, transport, chunk storage and player physics exist but nothing drives them together.

## What Changes

- New crate `mcr-client`: connection driver (handshake, login incl. compression and the online-mode encryption path, configuration, play, re-configuration, transfer), `ClientState`, a 20 TPS `Client::tick`, an actions API and a `ClientEvent` queue.
- New binary `mcr-headless` with scripted behaviours for live testing.
- Live `#[ignore]` integration test against the real vanilla server.

## Capabilities

### New Capabilities
- `client-connection`: protocol state machine from handshake to play, re-configuration, keep-alive, transfer, disconnect.
- `client-state`: registries, tags, dimension, chunks, entities, local player, inventory, world and UI state fed by clientbound packets.
- `client-tick`: fixed-step tick order, movement packets, actions API, chunk batch pacing, events.
- `headless-client`: the `mcr-headless` binary.

### Modified Capabilities
None.

## Impact

New crate `crates/mcr-client`, depends on mcr-net, mcr-protocol, mcr-world, mcr-physics, mcr-data, mcr-auth, mcr-nbt. No changes to other crates except small additive fixes if required.

## Out of scope

Rendering, audio, UI, signed chat and chat session keys, online-mode authentication flow (wired, untested), vehicle movement, recipe book, advancements, merchant/map/book UI state, command-tree completion, interpolation beyond exposing targets, hashing of non-empty component patches in container clicks.
