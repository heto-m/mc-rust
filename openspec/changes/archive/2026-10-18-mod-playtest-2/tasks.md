## 1. Change

- [x] 1.1 Proposal, design, specs and tasks; `openspec validate mod-playtest-2 --strict`

## 2. Ocean blink

- [x] 2.1 Review section uploads, translucent sorting, depth, culling, merged water tops and mip selection
- [x] 2.2 Translucent draws switch only when their upload batch is written; unit and GPU tests
- [x] 2.3 Offscreen reproduction at render distance 32 over an ocean, with and without mods (flight, still camera,
  consecutive-frame diffs)
- [x] 2.4 Animated cutout sprites mipped as one strip with one coverage scale; real-jar kelp test; recapture shows
  no blink

## 3. Play set mods

- [x] 3.1 Searchables and SuperMartijn642's Config Lib from Modrinth into `.cache/play-mods` (not committed)
- [x] 3.2 Controlling's key search and a durability tooltip in mod mode

## 4. Difficulty in mod mode (ABI 19)

- [x] 4.1 `WORLD_LOCKED`, `difficulty`/`lockdifficulty` requests, forward list, `BRIDGE_VERSION`, ABI test
- [x] 4.2 Rust: console or packet change, hardcore and lock refusals, lock flag and `level.dat` lock after stop
- [x] 4.3 Java: owner difficulty controls, packets rerouted to `REPORT_WORLD`, lock flag on the level data
- [x] 4.4 Unit tests and a live check in a local world (change, lock, next session locked); `docs/mod-bridge.md`

## 5. Finish

- [x] 5.1 Merge main, take the next free ABI number, fmt, clippy, tests, archive
