# Tasks

## 1. Tables

- [x] 1.1 Fetch 26.2 jars and run its data generator into `.cache/versions/26.2` (not committed)
- [x] 1.2 `mcr-data-gen` diff: packet id maps per state and direction, registry maps for differing registries, block states by block and properties; unit tests on small report fixtures
- [x] 1.3 `cargo xtask version-tables --version <old>`; generate and commit `v26_2/tables.bin`; currency test that skips without `.cache`

## 2. Translator framework

- [x] 2.1 `version` module: `Wire`, `Translator`, actions, table reader, supported versions with protocol numbers, `Wire::for_protocol`
- [x] 2.2 Byte rewrite helpers with bounds (VarInt remap, copy, strings, lists, depth guard); hostile-input tests
- [x] 2.3 Handshake protocol rewrite; packet id remap for all states; drop and synthesize hooks
- [x] 2.4 Criterion bench: captured 26.3 play stream decoded directly vs through `Wire::Canonical`

## 3. 26.2 translator

- [x] 3.1 Packet-level differences from the 26.2/26.3 source comparison (animate/swing, explode, player action, punch/swing, others found)
- [x] 3.2 Shared codecs: chunk sections and block updates (block states, direct palette width), item stacks and data components, particles, entity data, sound holders
- [x] 3.3 Remaining registry carriers: add entity, level events, stats, commands argument types, recipes/slot displays, advancements, merchant offers
- [x] 3.4 Unit tests with hand-built 26.2 payloads for each rewritten packet, plus truncation tests

## 4. Client and app

- [x] 4.1 `mcr-client`: `WireChoice`, status probe, wire applied to every inbound and outbound frame, compression hook via the wire's id
- [x] 4.2 Mod mode refuses a non-canonical wire before login; test
- [x] 4.3 `mcr-app`: status ping and server list compatibility by supported protocols; connect passes the pinged protocol, auto for direct connects and transfers, canonical for singleplayer
- [x] 4.4 Launcher: supported versions load assets from the canonical jar; tests

## 5. Live verification

- [x] 5.1 26.2 vanilla server on port 25572: join, configuration, play, walk, break and place, chat, inventory, entity spawns, respawn (offscreen autopilot); fix what breaks
- [x] 5.2 26.3: protocol fixture tests, then one live join on port 25572 after stopping the 26.2 server
- [x] 5.3 Add 26.2 to `SUPPORTED_VERSIONS` only if 5.1 passes
- [x] 5.4 fmt, clippy, workspace tests; validate and archive the change
