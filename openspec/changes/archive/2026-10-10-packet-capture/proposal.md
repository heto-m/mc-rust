## Why

Packet decoders need byte-exact fixtures from the real vanilla server. Hand-built bytes only prove self-consistency; recorded traffic proves the layouts.

## What Changes

- Add `mcr-capture`, a binary that logs in to a server, walks handshake, login, configuration and play, and records every inbound frame to disk.
- Optionally spawn the managed vanilla server and feed it console commands from a script to provoke more packet kinds.
- Support a mode that answers `select_known_packs` with no packs so the server sends full registry data.

## Capabilities

### New Capabilities
- `packet-capture`: record real server traffic as per-packet fixture files with an index.

### Modified Capabilities

## Impact

New crate `crates/mcr-capture` depending on `mcr-net`, `mcr-buf`, `mcr-data`, `mcr-auth`. Output lives under the gitignored `.cache/captures/`.
