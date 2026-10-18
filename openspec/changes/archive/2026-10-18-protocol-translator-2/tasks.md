# Tasks

## 1. Recursion audit

- [x] 1.1 Audit every recursive and mutually recursive translator path and every loop bound; record findings in design.md
- [x] 1.2 Thread `Depth` through particles; copy text components and NBT with the raw skipper
- [x] 1.3 Hostile-input tests: nested particle items, nested containers and bundles, nested slot displays, deep text components and NBT, zero-progress loops

## 2. Framework

- [x] 2.1 `Session` passed to `Wire::inbound`/`outbound`; `Translated::Packets` with extra frames; `Cx` for rewrites with per-translator `Maps`
- [x] 2.2 Tables carry old names for named registries; translator renames; regenerate 26.2 tables
- [x] 2.3 `mcr-client` uses the session and handles multi-packet results in both directions; bench updated

## 3. 26.2 gaps

- [x] 3.1 Brewing stand totals from per-session window state
- [x] 3.2 Creative stacks walked back to 26.2 layouts for every changed component
- [x] 3.3 Level events 2002/2007/3003 followed by their 26.3 sound packets
- [x] 3.4 Pot sherds in block entity NBT (chunks, `block_entity_data`); `potion_contents` predicates
- [x] 3.5 Known core pack echoes the server's version
- [x] 3.6 Unit tests for each

## 4. 26.1.x

- [x] 4.1 Fetch 26.1.2, run its data generator into `.cache/versions/26.1.2`; generate and commit `v26_1_2/tables.bin`
- [x] 4.2 Direct translator reusing the 26.2 rewrites; rules for `login`, `login_finished`, `set_player_team`, `spectator_action`, attributes, block entity types, the `color` argument rename
- [x] 4.3 Unit tests with hand-built 26.1.2 payloads; table currency test

## 5. Live verification

- [x] 5.1 Vanilla 26.1.2 on port 25572: mcr-client live suite with `MCR_LIVE_ANY_VERSION=1`, offscreen app join; fix what breaks; stop the server
- [x] 5.2 Vanilla 26.2 on port 25572: same suite and app join; stop the server
- [x] 5.3 Canonical 26.3 regression run on port 25572; stop the server
- [x] 5.4 Add 26.1.x to `SUPPORTED_VERSIONS` only if 5.1 passes

## 6. Measurements and wrap-up

- [x] 6.1 Criterion bench: translated chunk and item stream per version; record numbers in design.md
- [x] 6.2 Record remaining gaps in design.md
- [x] 6.3 fmt, clippy, workspace tests; validate; archive
