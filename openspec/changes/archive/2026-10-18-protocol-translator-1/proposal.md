# Proposal

## Why

mcr speaks only 26.3 (protocol 777). The fixed plan is to reach older servers ViaVersion-style: translators
onto the one canonical 26.3 packet set, so the client never learns about versions. There is no translator
layer yet, so a 26.2 server just fails at login.

## What Changes

- **Translator layer** in `mcr_protocol::version`. A `Wire` value sits between the socket and the canonical
  codecs for every state and both directions. `Wire::Canonical` (26.3) passes frames through unchanged with no
  per-packet work. `Wire::Legacy(&Translator)` remaps packet ids, rewrites payloads whose fields differ, remaps
  registry ids (block states, items, entity types, sounds, particles, data components, stats, command argument
  types) and synthesizes or drops packets that exist in only one version, each with a defined fallback. All
  rewriting is bounded like any other hostile input.
- **Generated diff tables.** `cargo xtask version-tables --version <old>` reads both versions' data generator
  reports and writes a compact table file (packet and registry id maps, block states matched by block and
  properties) into `mcr-protocol`. The tables are id facts, committed like `mcr-data`'s blob. A test checks
  they match the reports when `.cache` has them.
- **26.2 (protocol 776)**, the newest release before 26.3, translated end to end and checked live against a
  vanilla 26.2 server.
- **Protocol choice.** The status ping reads the server's protocol; the server list calls servers on any
  supported protocol compatible. Joining picks the translator from that protocol, probing with a status ping
  when it is not known yet (direct connect, transfer). Singleplayer stays canonical.
- **Launcher.** Supported older versions become selectable. Their assets still come from the 26.3 jar.
- **Mod mode** refuses servers that are not on 26.3, with a clear message.
- **Benchmarks** of the canonical path against plain decoding.

## Out of scope

Versions older than 26.2, mod mode on older servers, older assets or resource formats (26.3 assets always),
chat signing differences beyond what 26.2 needs, and the reverse direction (a 26.2 client on mcr's server).

## Capabilities

### New Capabilities
- `protocol-translation`: the `Wire` layer, translator contract, generated tables, the 26.2 translator.

### Modified Capabilities
- `server-list`: compatibility from the supported protocol list.
- `client-connection`: wire chosen from the server's protocol; probe when unknown.
- `launcher`: supported older versions selectable, assets from the canonical jar.
- `mod-net-bridge`: older servers refused.
- `dev-tooling`: `version-tables` command.

## Impact

`mcr-protocol` (version layer, tables, benches), `mcr-data-gen` (diff generator), `xtask`, `mcr-client`
(wire in the connection), `mcr-app` (status, server list, connect), `mcr-launcher`.
