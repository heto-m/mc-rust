## Why

The client cannot connect to a server until the handshake, status, login and configuration packets
exist. Packets shared between configuration and play and the real text component model are
prerequisites for every later protocol area.

## What Changes

- Add handshake, status, login and configuration packet types for protocol 777, both directions.
- Add shared common packets (keep-alive, ping/pong, custom payload, disconnect, cookies, transfer,
  resource packs, tags, server links, dialogs, client information, custom click action) once, for
  reuse by configuration and play.
- Replace the placeholder `Component` with a full text component model (content, style, events,
  siblings) that round-trips through network NBT and decodes leniently.
- Add `GameProfile`, `KnownPack`, registry-data entries and the tag payload types.

## Capabilities

### New Capabilities
- `protocol-handshake-config`: packet definitions and wire encodings for handshake, status, login,
  configuration and the shared common packets.
- `text-components`: text component model with network NBT encode/decode and plain-text rendering.

### Modified Capabilities

## Impact

- `crates/mcr-protocol`: `handshake`, `status`, `login`, `configuration`, `common`,
  `types::{text, profile, registry}`.
- No new dependencies.
