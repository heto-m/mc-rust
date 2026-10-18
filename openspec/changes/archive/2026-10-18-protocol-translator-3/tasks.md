# Tasks

## 1. Survey

- [x] 1.1 Fetch 1.21.11, run its data generator; diff packets, registries, components, entity data, chunk format, configuration and text against 26.1.2 and 26.3; record in design.md
- [x] 1.2 Decide direct versus chain; record the reasoning in design.md

## 2. Framework

- [x] 2.1 `Era` and the `slot_display` remap in `Maps`; translators pass their era
- [x] 2.2 Item codec: stacks versus templates, `EitherHolder`s, tag keys, both directions; dropped components keep counts right
- [x] 2.3 Session: bounded ageable entity id set and sneak state
- [x] 2.4 Chunk sections: computed fluid counts for 1.21.11, lane-parallel count
- [x] 2.5 Hash dialect on `Wire`; client hashes stacks and pot decorations in the server's dialect

## 3. 1.21.11 translator

- [x] 3.1 Generate and commit `v1_21_11/tables.bin`; table currency test
- [x] 3.2 Rules: `set_time`, `add_entity`, `remove_entities`, `login`, `respawn`, `set_entity_data`, `interact`, `attack`, `spectator_action`, `player_input`, `client_command`, plus the missing loom item tags in `update_tags`
- [x] 3.3 Unit tests with hand-built 1.21.11 payloads for each rule and changed component
- [x] 3.4 Hostile-input tests: random payload fuzz over every rule, depth limits through templates and containers, huge counts, a full ageable set

## 4. Live verification

- [x] 4.1 Vanilla 1.21.11 on port 25572: mcr-client live suite with `MCR_LIVE_ANY_VERSION=1`; fix what breaks
- [x] 4.2 Offscreen app join with screenshots in `run/`
- [x] 4.3 Canonical 26.3 regression run on port 25572
- [x] 4.4 Add 1.21.11 to `SUPPORTED_VERSIONS` only if 4.1 passes

## 5. Measurements and wrap-up

- [x] 5.1 Criterion bench: 1.21.11 chunk, inventory and entity frames next to 26.2 and 26.1.2; record in design.md
- [x] 5.2 Asset comparison 1.21.11 against 26.3; record visual differences in design.md
- [x] 5.3 Record remaining gaps; fmt, clippy, workspace tests; validate; archive
