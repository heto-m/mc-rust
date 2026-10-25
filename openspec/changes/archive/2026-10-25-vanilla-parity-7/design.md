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

## Implementation notes

#### Q1 and Q2 F3 entries
- `DebugScreenOverlay` is followed as written: entries run in id order (`currentlyEnabled` is sorted), each writes
  a priority line (alternating left and right), a regular line, or lines into a named group; regular lines then
  groups split into halves, a group counting even when empty. `debug.rs` has the same `Displayer`.
- 28 text entries now (was 15; 30 with the two renderers). Added: `detailed_memory`, `gpu_utilization`,
  `heightmap`, `looking_at_block_tags`, `looking_at_entity_tags`, `looking_at_fluid_state`,
  `looking_at_fluid_tags`, `particle_render_stats`, `player_speed`, `post_effects`,
  `simple_performance_impactors`, `sound_cache`, `sound_mood`. Fluid targets come from the eye ray at 20 blocks
  against block and fluid shapes (`ClientState::fluid_pick`); tags from the synced tag registries.
- Not added, because they read a `ServerLevel`: `local_difficulty`, `entity_spawn_counts` and
  `chunk_generation_stats`. The `heightmap` server line prints `??` as vanilla does without a server chunk.
- Memory lines read `/proc`: heap is the anonymous resident set, non-heap the file-backed one. The position
  group is vanilla's (region-local chunk, `FC`, section-relative).

#### Q3 World Options scroll
- Rows sit in a viewport from y 31 to 28 px above the bottom, under the title and over Apply and Cancel. They
  scroll with the wheel or the scrollbar and follow keyboard focus; a click outside the viewport never reaches
  a clipped row. From a 240 px GUI nothing scrolls, so the screen is unchanged there.

#### Q4 Cloud depth target
- With improved transparency on, fancy clouds draw in a pass of their own per stage against a screen-sized
  depth target: the depth bounds stage copies the opaque depth into it and writes the nearest face, the other
  two stages test against it, so one face shades a pixel. The vertex position is `@invariant` so the stages
  agree. Flat clouds and the classic path are unchanged.
- GPU medians, 1920x1080, a flat world, 600 frames, 3 interleaved rounds, ms: looking up from below the layer
  0.214 before, 0.247 after; inside the layer 0.200 before, 0.233 after. The 0.033 ms is the depth copy and
  three extra passes, constant with the view.

#### Live checks (vanilla 26.3 server on loopback in a network namespace, offscreen)
- Q6 found three bugs in the LAN flow, all fixed with tests:
  - A guest client could not join: it pings status first, and a managed world answers none, so the join died
    with "connection closed by peer". The join now takes a server that closes the ping as current.
  - Command access and the forced game mode were sent when the gate admitted the guest, before the player was
    in the world and with the gate's lowercase name, so `op` named a different offline UUID and the guest got
    no commands. They are sent once the guest is in the host's player list, under the name it joined with.
  - Reopening World Options on a shared world flagged the world's own port as unavailable and disabled Apply.
- With those: LAN opens on the typed port; a guest joins and, with Command Access on, runs `/time set night`;
  turning it off deops the guest and `/time set day` is refused; Force Game Mode on with a new default mode
  sets the present guest's mode; LAN off closes the guest's connection.
- Q7: the autopilot has a `drop <path>` step that calls the shell's `dropped_file`, as winit does. With the
  game's `resourcepacks` a plain file, confirming the drop shows the "Failed to copy packs" toast with the folder.
- Q5 stays open: `stop_sound` in mod mode needs the Fabric JVM and mods are paused. Q8 needs an online account.

## Close-out notes

Carried into the next `vanilla-parity-N` change:

- Q5 stop_sound in mod mode, once mods resume.
- Q8 cape on elytra, which needs the owner's online-mode account.
- The `local_difficulty`, `entity_spawn_counts` and `chunk_generation_stats` debug entries. They need a
  server level, and the managed singleplayer server runs out of process.
- `detailed_memory` now reads `/proc/meminfo`. It is not yet rechecked on the F3 screen.
