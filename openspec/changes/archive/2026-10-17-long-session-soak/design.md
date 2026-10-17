## Context

`vanilla-polish-3` freed texture-array layers and listed the texture allocators it checked. This change audits everything else that lives longer than a frame and adds a soak harness that measures memory over a real session, so "leak-free" is a measurement rather than a reading of the code.

## Audit method

Every long-lived collection, GPU allocation, thread spawn and channel was traced from its insert to its removal and checked against its natural lifecycle: entity removal, chunk unload, player leave, respawn and dimension change, reconfiguration, disconnect and reconnect. The tables below list what grew and how it is bounded now. Each fix has a unit test that churns it far past its bound.

## Lifecycle: what owns world state

- `Shell` lives for the process. It keeps `Parts` (renderer, GUI and UI atlas, assets, audio, settings) and lends them to each session.
- `Play` lives for one connection. Disconnect and transfer drop it, and `Play::finish` hands `Parts` back.
- A world reset is `Respawned`, `Reconfiguring` or `Joined`. The client sends these events, and `net_world::resets_world` names them. Before this change the app only handled `Respawned`. Reconfiguration (proxy server switches) left the old world's columns, GPU sections and block entities behind.

## Leaks found and fixed

| Where | Growth | Fix |
|---|---|---|
| Renderer sections on dimension change | Sections were removed with the *new* dimension's height range, so overworld sections below y 0 and above y 256 stayed after entering the Nether or the End, in the arena and the visibility graph, into later sessions | `Renderer::clear_sections` on every world reset and in `into_parts` |
| Renderer sections on disconnect | Columns already queued for unload were skipped by `into_parts` | Same: the renderer clears every section it holds |
| App state on reconfiguration | `NetworkWorld.loaded`, GPU sections, block entities, entity tracks and effects were kept for the old world | Reconfiguring and Joined reset the app like a respawn |
| Client `world.clocks` | Server-keyed, never cleared, no total cap | Capped at 64; cleared with the world |
| Client `server.game_rules` | Server-keyed, never cleared, values up to 32767 chars | Capped at 1024, values cut to 256; cleared on reconfiguration |
| Client chunk store | The centre sweep scanned only around the *new* centre, so a jump of more than about 70 chunks without forget packets stranded the old area; a smaller radius evicted nothing | Sweep every stored column on centre and radius changes; the range check no longer overflows on hostile coordinates |
| Client state on reconfiguration | Camera entity (froze the player), predictions, cooldowns, gameplay state (recipe book, advancements, maps, stats, waypoints), controlled vehicle | Reset in `reset_for_reconfiguration`; predictions, clocks and vehicle also on dimension change |
| Dialog and conduct channels | Unbounded mpsc; the app reads 16 dialogs and 1 conduct request per frame | Bounded `sync_channel`s (64 and 4) with `try_send` |
| UI atlas: server-list favicons | Each opening of the multiplayer screen uploaded every favicon again and never freed them | `SpriteReleaser`: the screen's `Drop` hands its ids back, freed on the next `add_raw` |
| UI atlas: HUD heads, vignette, spyglass | Allocated per session in the process-lifetime atlas, never freed | `OwnedSprites` returns them when the session's `Hud` and `Overlays` drop |
| Audio decoder threads | One thread per streaming voice, up to the 247-voice cap | `StreamPermit`: 8 streams at once, like vanilla's streaming pool |
| Menu music events | Never drained in menus | Kept to the latest 16 |
| Decoded sound cache | Every non-streamed sound ever played stayed decoded as f32 PCM; new biomes, mobs and places kept adding to it (about 10 MiB/min in the soak, found with a jemalloc heap profile after the static audit missed it) | 64 MiB budget, least recently used evicted first; sounds over 16 MiB are not cached |
| Pack downloads | One detached thread per batch; results from a previous session applied to the next | One download thread, 16 waiting batches (more fail at once), results tagged by session |
| Server-list pings | `refresh` and `remove` reset the in-flight count, so each refresh could start 8 more threads | Old pings keep their slot until they answer |
| Opener processes | `xdg-open` children never waited on, one zombie per click | Reaped on a short-lived thread |
| Crack decals | Entries for breakers that left mid-dig never expired, filling all 128 slots | Expire 400 ticks after the last update, like vanilla |
| Entity textures | Unresolvable keys (inline painting variants) cached forever | At most 4096 remembered misses |
| Equipment assets | Every unknown `equippable.asset_id` cached as `None` forever | At most 1024 remembered misses |
| Flat renderer groups | One group per text-display rank and page, never removed; push cost grew with them | Groups empty for a frame are dropped; the quad count is a counter |
| Mod bridge `SubmitTrack.inflight` | One entry per submit; pruned only in rare waits | Completed submits pruned on every record |
| Mod bridge upload ring | The two kept blocks kept their first size, up to 512 MiB each (host and GPU) | Idle blocks larger than 4 MiB are released |
| Mod bridge byte pool | Up to 64 buffers of 16 MiB | At most 64 MiB idle in all |
| Mod bridge server camera | A static strong reference to the spectated entity kept a left world alive | `WeakReference` |
| Net buffer pools | Up to 256 idle buffers of 1 MiB per pool | At most 8 MiB idle per pool |

## Checked and bounded

- Client tables: entities, passengers, effects and attributes, players, scoreboard, teams, boss bars, chat, registries, recipes, advancements, maps, stats, waypoints, cookies, inventory, pickups, transient blocks. Each is capped and cleaned on its event.
- App: entity tracks, text-display layouts, entity event timers, entity sound loops, ambience, skins (pruned to present players), effects trackers and explosions, records, render queues, post chains, screen stacks and toasts.
- Render: terrain arenas are fixed-size with coalescing free lists. There is no staging belt; uploads go through `queue.write_buffer`. Readback buffers are fixed. Mesh workers are a fixed pool joined on drop. Entity bone, instance and flat buffers grow to a capped high-water mark.
- Audio: voices are removed when finished and the command and ended rings are fixed.
- Mod bridge: GPU handles freed through Java's `close()`, bind group cache capped at 8192, readbacks capped, net rings fixed with a 256 MiB spill cap, PCM table capped, Java input and event queues capped.
- `mod-camera-weather`: the shared reached-sections block is one 1 MiB allocation for the process (`SectionShared` in the global `Shared`), and the camera report is a fixed set of atomics. The graph's reached bitmap is sized to the clamped grid box. The Java side keeps no growing collections; the one leak was the static server camera reference above.

## Left as is

- The outbound frame channel in `mcr-net` is unbounded. It only grows when a server reads slowly but steadily, at the rate the client sends (a few KB/s).
- Free lists of the UI atlas are per sprite size, so cycling GUI scales leaves free slots of every scale's icon size. The total is bounded by 1024 icons per size.
- `IconCache` in the mod manager keeps sprite ids across a GUI reload. That is a correctness bug, not a leak.

## GPU counters

The workspace enables wgpu's `counters` feature. `Renderer::gpu_counters` reads the HAL counters: live buffers, buffer bytes, texture bytes, texture views, bind groups. These cover every renderer on the device, including the mod bridge, at the cost of a relaxed atomic per create and destroy. `EntityScene::live_layers` adds the entity texture layers. The HAL texture *count* goes negative on Vulkan in wgpu 30 (destroying subtracts, creating does not add), so the soak uses texture bytes.

## Soak harness

`cargo xtask soak` runs a dedicated server in `run/soak-server-<ver>` with a fresh world for each run:
- fixed seed, view distance 8, creative, whitelist off, `max-tick-time=-1`, and `immediate_respawn` set once the client joins
- its console is driven through a pipe from xtask

`mcr --soak` runs in its own `MemoryMax=3G` scope. A real leak is killed inside that scope (exit 137) and the soak fails.

Each 60-second cycle:
- second 0: summons a wave (800 by default) of 14 entity kinds, including all three display kinds, items, minecarts and named mobs
- seconds 20 and 50: kills everything except players
- spams chat and `tellraw`
- sets a title, subtitle and action bar
- adds a boss bar and removes the previous one; adds an objective and a team with 15 scores and removes the previous ones
- gives 7 items with components, then clears them
- places a row of signs and banners, then clears it
- particles, sounds and a music disc
- weather and time
- second 30: teleports to one of 10 places up to 20000 blocks away, every fourth cycle into the Nether and every fourth into the End
- `/reload` every fourth cycle

Kicks and transfers alternate, spread over the run after the warm-up. mcr rejoins three seconds after a session ends.

Every 10 seconds the harness samples VmRSS and Threads from `/proc/<pid>/status` and the latest `soak-stats` line. With `--mods`, it also reads the heap after the last collection from a `-Xlog:gc` file passed through `JAVA_TOOL_OPTIONS`. The verdict uses least-squares slopes over the samples after warm-up (5 minutes by default). It fails over 1 MiB/min of RSS, GPU bytes or JVM heap, or when the median thread count of the last third exceeds that of the first third.

The baseline binary is main (`a76324c`) plus only the harness commits (counters, soak mode, respawn), built in a detached worktree.

## Allocator retention

With the sound cache bounded, a jemalloc heap profile of a 15-minute soak showed the live heap flat (300.1 MB at minute 5, 300.2 MB at minute 15), yet glibc's RSS still climbed about 3 MiB/min. glibc raises its mmap threshold each time an mmapped block is freed, so the cache's evicted multi-megabyte buffers moved into the main heap and stayed there as fragments. `mcr` pins `M_MMAP_THRESHOLD` to 256 KiB at startup (glibc targets only), which keeps large buffers in their own mappings and returns them on free.

## Results

20-minute soaks, 800-entity waves, three kicks and transfers, slopes over minutes 5 to 20. RSS is mcr's VmRSS; GPU is wgpu's live buffer plus texture bytes.

| Run | RSS start → end (peak) | RSS slope | GPU slope | Threads |
|---|---|---|---|---|
| Baseline (`a76324c` + harness) | 238 → 469 MiB (471) | +9.37 MiB/min | +0.32 MiB/min | 48 → 48 |
| Audit fixes | 237 → 471 MiB (472) | +10.44 MiB/min | −0.01 MiB/min | 48 → 48 |
| + bounded sound cache | 240 → 381 MiB (381) | +3.07 MiB/min | +0.17 MiB/min | 48 → 48 |
| + pinned mmap threshold (final) | 234 → 324 MiB (349) | −1.74 MiB/min | +0.01 MiB/min | 48 → 48 |

The audit fixes flattened GPU memory (texture bytes grew 25 → 48 MiB in the baseline, 25 → 44 MiB and level after) but not RSS, which the static audit had wrongly counted as bounded by `sounds.json`. The final run passes; RSS rises in the first minutes as the sound cache fills to its budget, then moves between 320 and 350 MiB.

Mod mode, 10 minutes with Fabric API and Mod Menu (JVM heap pinned at 1024 MiB): RSS 1969 → 2138 MiB (peak 2189), slope −7.35 MiB/min; GPU slope −0.12 MiB/min; threads 112 → 110; Java heap after collection between 338 and 402 MiB, slope −4.62 MiB/min. Pass.
