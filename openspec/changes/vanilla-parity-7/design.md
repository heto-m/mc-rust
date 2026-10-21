## Context

Leftovers from the close-out of `vanilla-parity-6` (its "Close-out notes" and live-check notes). Vanilla
references are `Class#method` in the decompiled 26.3 client under `.cache/versions/26.3/src/`. Severity:
**G** gameplay-breaking, **V** visible, **M** minor.

## Gap catalogue

### F3 debug entries

#### Q1 Debug entries are a subset (V)
- Vanilla: `DebugScreenEntries` registers 43 entries.
- mcr: 17 (`crates/mcr-ui/src/gui/debug_entries.rs`). Missing with data this client has or can derive:
  `detailed_memory`, `looking_at_block_tags`, `looking_at_fluid_state`, `looking_at_fluid_tags`,
  `looking_at_entity_tags`, `player_speed`, `heightmap`, `local_difficulty`, `particle_render_stats`,
  `entity_spawn_counts`, `sound_mood`, `sound_cache`, `post_effects`, `gpu_utilization`,
  `simple_performance_impactors`, and `chunk_generation_stats` (local worlds only).
- Impact: the F3 screen lacks lines players use (tags, speed, local difficulty).

#### Q2 F3 text order (M)
- Vanilla: `DebugScreenOverlay` lets each entry add lines to a group; groups keep the order they are
  first written and split into the left and right columns.
- mcr: a fixed grouping (version, timing, chunks, position, world, memory, specs, target).

### World Options and clouds

#### Q3 World Options page does not scroll (M)
- Vanilla: `WorldOptionsScreen` puts the rows in a `ScrollableLayout`.
- mcr: the rows fit from a 240 px high GUI only; smaller GUIs clip them.

#### Q4 No cloud depth target with improved transparency (M)
- Vanilla: fancy clouds write a separate depth target, so only the nearest cloud face contributes.
- mcr: every front face contributes, as in the classic path.

### Live checks

#### Q5 stop_sound in mod mode untested live (M)
- `stop_sound` is Java-owned in mod mode since `vanilla-parity-5` L22; only unit-tested. Needs the Fabric JVM.

#### Q6 Local-world World Options rows untested live (M)
- LAN open and close, Command Access and Force Game Mode need a managed singleplayer server with a guest.

#### Q7 Pack drop toast untested live (M)
- Needs a window file drop that fails to copy.

#### Q8 Cape on elytra untested live (M)
- Needs an online-mode account: owner only.

## Severity totals

| Batch | G | V | M | Total |
|---|---|---|---|---|
| 1 F3 debug entries | 0 | 1 | 1 | 2 |
| 2 World Options scroll and cloud depth | 0 | 0 | 2 | 2 |
| 3 Live checks | 0 | 0 | 4 | 4 |
| **Total** | **0** | **1** | **7** | **8** |
