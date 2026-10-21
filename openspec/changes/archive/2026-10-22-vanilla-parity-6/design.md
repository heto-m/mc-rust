## Context

Leftovers collected from the design notes of `vanilla-parity-5` (Batch 4 and Batch 8 notes, per-gap fix
notes and live-check notes). Vanilla references are `Class#method` in the decompiled 26.3 client under
`.cache/versions/26.3/src/`. Severity: **G** gameplay-breaking, **V** visible, **M** minor.

Spot-checked against the decompiled client: `WingsLayer` still has a `skin.elytra()` texture path;
`BeaconBlockEntity#tick` recounts the base when `getGameTime() % 80 == 0` and scans the beam
incrementally with `lastCheckY`; `LevelRenderer` branches on `useImprovedTransparency()` around
`executeClassicTransparency`.

## Gap catalogue

### Fabulous transparency completion

#### P1 Translucent entity layers and item entities draw classic (V)
- Vanilla: with improved transparency, `LevelRenderer#renderLevel` (`useImprovedTransparency`) routes
  translucent entity and item entity features through the transparency chain.
- mcr: they draw after the composite (Batch 8 notes).
- Impact: a translucent entity behind water or glass sorts wrong.

#### P2 Weather, clouds and world border draw after the composite (V)
- Vanilla: the same chain composites weather, clouds and the world border.
- mcr: drawn after the composite, so they ignore depth against translucent terrain.

#### P3 Composite writes no depth (M)
- Vanilla: `LevelRenderer#renderLevel` keeps depth from the composited targets for later passes.
- mcr: the composite writes none, so later passes depth-test against opaque depth only.

#### P4 Dynamic lights do not reach the OIT terrain pass (M)
- Vanilla: `LevelRenderer` applies the light sources to all terrain passes.
- mcr: the OIT terrain pass skips them, so translucent terrain near a held torch is unlit by it.

### Skin elytra texture and live checks

#### P5 Skin ELYTRA profile texture not fetched (V)
- Vanilla: `WingsLayer#getPlayerElytraTexture` prefers `skin.elytra()`, else the cape.
- mcr: wings use the cape when shown, else the item texture; no `ELYTRA` texture is fetched from the
  profile (`crates/mcr-entity-model/src/humanoids.rs`, `crates/mcr-auth`).

#### P6 Cape on elytra untested live (M)
- Offline players have no cape, so only an offscreen test covers it. Needs an online-mode check.

#### P7 Map label order untested live (M)
- Vanilla: `MapRenderer#render` draws names at one depth level with the 36th icon. Covered offscreen
  only because the live check needs 37 named banner markers. Needs a live check.

### Terrain late-phase cost and beacon beam cadence

#### P8 Disoccluded sections shade leaves behind terrain (M, performance)
- Vanilla: SOLID before CUTOUT for every section.
- mcr: Batch 8 split solid and cutout draws in the early cull phase only; late-phase (disoccluded)
  sections still draw in one discarding pass (`crates/mcr-render/src/shaders/cull.wgsl`).
- Fix direction: split the late phase too; frames must stay pixel-identical.

#### P9 Beacon beam rescan every 20 client ticks (M)
- Vanilla: `BeaconBlockEntity#tick` scans the beam one step per tick via `lastCheckY` and recounts the
  base every 80 game ticks.
- mcr: the base cadence matches since Batch 8; the beam still rescans every 20 client ticks
  (`crates/mcr-app/src/block_entities.rs`).
- Impact: a beam extends or shortens up to 1 s later than vanilla.

## Declined

- L25 texture filtering NONE at mip seams: vanilla's `sampleNearest` on a linear sampler cost 14 to 21%
  more GPU in every view, measured in `vanilla-parity-5`. Hardware nearest stays.

## Severity totals

| Batch | G | V | M | Total |
|---|---|---|---|---|
| 1 Fabulous transparency completion | 0 | 2 | 2 | 4 |
| 2 Skin elytra texture and live checks | 0 | 1 | 2 | 3 |
| 3 Terrain late-phase cost and beacon cadence | 0 | 0 | 2 | 2 |
| **Total** | **0** | **3** | **6** | **9** |

### World Options and debug screens (from Batch 5-7 notes)

#### P10 World Options rows missing (V)
- Vanilla: `WorldOptionsScreen` (26.3) has Restrictions, guest commands and force game mode rows with tooltips.
- mcr: difficulty and lock, game rules, game modes, allow commands and LAN only.

#### P11 LAN cannot be closed (M)
- Vanilla: the LAN row toggles publishing both ways.
- mcr: it opens LAN but has no way to stop it.

#### P12 Default game mode starts from the host's mode (M)
- Vanilla: reads the world's default game mode.
- mcr: the client doesn't track it, so the row starts from the host's current mode.

#### P13 Debug screens simplified (M)
- Vanilla: `DebugOptionsScreen` (F3+F6) with entries and profiles; F3+L writes a profiling zip; F3+1 shows the profiler pie.
- mcr: a toggle list, a text summary, and frame and tick charts.

#### P14 Pack drop failures are silent (M)
- Vanilla: `PackSelectionScreen` shows a toast when copying a dropped pack fails.
- mcr: errors go to stderr.

#### P15 Untested live (M)
- Spear hit dip, distance-delayed level events 3012-3021 and stop_sound in mod mode are unit-tested only.

## Implementation notes (batch 2 and batch 4)

#### P5 Elytra texture
- `parse_textures` reads `ELYTRA` from the trusted texture host only, exactly like `CAPE`; the PNG goes through the same 256 KiB, 64..=1024 px bounded download. `WingsLayer` order: the profile elytra, else the cape while the cape part shows, else the item texture.

#### P10 to P12 World Options
- Rows follow `WorldOptionsScreen`: Restrictions (a `RestrictionsScreen` that derives its limits from the chat visibility option; the launcher and profile limits need an online account and are not modelled), Command Access and Force Game Mode with vanilla's enable/force rules and tooltips, plus tooltips on the difficulty, game rules, game mode and allow commands rows.
- The page does not scroll (vanilla's does); the rows fit from a 240 px high GUI.
- The managed vanilla dedicated server has no command for guest command access or forced game mode, so the client applies them through the console to the guests the LAN forwarder lists: `op` or `gamemode <default> <guest>` when a guest joins or the setting changes, `deop` when they leave or LAN closes. Force Game Mode defaults on, as in `IntegratedServer`.
- Closing LAN stops the forwarder and announcer; changing the port of an open LAN closes and reopens it.
- The default game mode comes from the world (`level.dat` or the new-world settings) and is tracked by the session after each change.

#### P13 Debug screens
- Entries: only entries this client has data for are listed (game_version, fps, tps, memory, system_specs, player_position, player_section_position, chunk_render_stats, chunk_source_stats, entity_render_stats, light_levels, biome, day_count, looking_at_block_state, looking_at_entity, entity_hitboxes, chunk_borders). Unlisted vanilla ids found in `debug-profile.json` are kept and written back. Profiles Default and Performance match `DebugScreenEntries.PROFILES`; the file is `debug-profile.json` in vanilla's shape (`{"profile": ..}` or `{"custom": {..}}`, `DataVersion` omitted).
- Text order: our fixed grouping (version, timing, chunks, position, world, then memory, specs, target) instead of vanilla's per-entry priorities. `tps` carries the client tick and the server brand and player count, since a vanilla server's tick time is not sent; `day_count` carries the dimension and game time.
- Reduced debug info hides the same entries it does in vanilla (`reduced_ok` on the entry).
- The screen has the search box, the warning, category headers, Off / In Overlay / Always buttons, tooltips for hidden entries, the two profile buttons and Done. No debug GUI scale button (the option does not exist here). Advanced tooltips, FPS and network charts, lightmap and profiler chart are not entries in vanilla either; they stay on their F3 chords.
- F3+L writes `debug/profiling/<stamp>-<world>-<version>.zip` with `system.txt`, `client/options.txt`, `client/metrics.txt`, `client/sections.txt` and the frame and tick samples as CSV. Vanilla's zip holds its own sampler and profiler formats; ours keeps what this client measures.
- F3+1 draws the profiler pie of five flat sections (tick, update, visuals, gui, draw) smoothed over frames, with vanilla's list layout and colour rule. The ellipse is a flat disc with a darker wall, and the digit keys do not descend into the tree because there is no nested profile.

#### P14 Pack drop toast
- Copy failures raise vanilla's `pack.copyFailure` system toast naming the pack folder, once per drop, through a `SystemToast` screen action.

#### P7 Map labels (live check)
- 37 named banner markers on a vanilla 26.3 server showed three bugs the offscreen test could not: only 32 names were baked (`MAX_LABELS` is now 256, the decoration cap); item frames drew the picture and names rotated 180 degrees (the quad already carries `MapRenderer`'s Z rotation, so the frame's extra one went); and the markers' x offset was mirrored against the picture (`cx` is now negative east). The ordering check passes: the 36th and 37th icons cover the names, the earlier ones sit under them.

#### Live checks (batch 2 and 4), vanilla 26.3 server on loopback
- P7: 37 named banners and a painted map in an item frame, screenshots offscreen. P13: F3+F6 screen, entry buttons, Performance profile (`debug-profile.json` written as `{"profile":"performance"}`), F3+1 pie and an F3+L zip (six files). World Options and Restrictions on a remote world (op). F3+F6 used to toggle the overlay on F3's release because opening the screen reset the chord state; fixed with `keep_chord`.
- Not live: the local-world rows (LAN, Command Access, Force Game Mode) and the pack drop toast, which need a managed singleplayer server and a window file drop; both are unit-tested. P6 needs an online account.

## Implementation notes (batch 1 and batch 3)

GPU medians at 1920x1080, render distance 16, 600 frames, 3 interleaved rounds (ms; the forest, canopy,
beach and edge views of the parity-5 Batch 8 notes):

| Build | Forest | Canopy | Beach | Edge |
|---|---|---|---|---|
| Before, improved transparency off | 0.237 | 0.139 | 0.155 | 0.167 |
| After, improved transparency off | 0.237 | 0.139 | 0.155 | 0.167 |
| Before, improved transparency on | 0.364 | 0.254 | 0.334 | 0.331 |
| After, improved transparency on | 0.368 | 0.259 | 0.344 | 0.337 |

With the option off the frames are byte-identical to before. With it on, the 4 to 10 µs are the composite's
depth write (most on the beach, where most pixels hold water); these views have no entities, weather or
border.

#### P1 Entities, items and block entities
- `EntityRenderer` builds stage pipelines (`entity_oit.wgsl`) on first use: blended parts as `OIT_ENTITY`;
  additive parts (glint, eyes, energy swirl) as `OIT_ADDITIVE`, which bound depth and add light behind
  what lies in front but have no transmittance stage. Decals (armor trims) sit on cutout parts, so they
  draw with the opaque parts inside the world pass. Block entities route beam glow, sign text and text
  displays through the stages (`flat_oit.wgsl`, the display offset kept as depth bias); see-through text
  stays on top after the composite, as vanilla's see-through phase.
- Items: vanilla picks `item_translucent` for quads whose sprite has translucent pixels
  (`BakedQuad.MaterialInfo#of`). A texture now records that when it is uploaded, and item parts on such a
  texture draw translucent, so stained glass, ice, slime and honey items blend in both modes and go
  through the stages with the option on.
- The block outline draws before the stages (vanilla's `shapeOutlines` go through them), so translucent
  surfaces in front cover it instead of the composite depth hiding it.

#### P2 Border, weather and clouds
- Drawn in every stage after the overlay's work: rain and snow and clouds as translucent, the border as
  additive. Crumbling decals stay classic and draw before the stages, over the opaque blocks they crack.
- Not modelled: vanilla's separate cloud depth target, which lets only the nearest fancy-cloud face
  contribute; every front face contributes here, as in mcr's classic path.

#### P3 Composite depth
- The composite writes the nearest translucent depth (bounds `b`) like `oit_composite.fsh`, tested
  `GreaterEqual` instead of vanilla's always-pass: identical wherever coverage exists, and pixels lit only by
  near-transparent additive surfaces keep the opaque depth instead of the far plane.

#### P4 Dynamic lights
- With dynamic lights built, the accumulate stage uses a variant (`terrain_oit_dyn.wgsl`, light grid at group
  2, stage inputs at group 3); the other two stages only need alpha.

#### P8 Late phase
- The late cull phase splits each disoccluded section into a solid list and a cutout list like the early
  phase: four command regions (early solid, late solid, early cutout, late cutout) and four more counters.
  Frames are byte-identical in all four culling modes on all four views, first frames included. GPU medians
  unchanged (within 0.001 ms in every mode): steady views disocclude few sections.

#### P9 Beacon beam
- `BeamScan` follows `BeaconBlockEntity#tick`: up to 10 blocks a tick from the beacon to the column's
  `WORLD_SURFACE` height, publishing the sections when it gets there, then starting over; the pyramid
  recount uses the published beam. The surface is read from the blocks because the client's heightmaps are
  only as fresh as the chunk packet. Hostile bounds: 256 beacons step per tick (round robin), heights are
  checked in 64-bit and a scan is capped at `MAX_BEAM_HEIGHT`; parity-5's recount clock is unchanged.

#### Live checks (batch 1 and 3), vanilla 26.3 server on loopback, offscreen
- A slime and a lime glass item between light blue glass and water, a red glass item and signs in front:
  with the option on the slime and item behind the glass take its tint and no longer draw over it; clouds and
  the world border behind the glass are attenuated by it instead of covering it; rain blends in both modes.
- A level 1 beacon: placing red stained glass three blocks above it turns the beam above the glass red.

## Close-out notes (batch 5)

#### P15 live checks, vanilla 26.3 server on loopback, gamescope headless
- Spear dip: an iron spear with a patched `kinetic_weapon` (damage condition only, no speed minimum) held
  in use against a NoAI zombie three blocks ahead. The server stabbed every 10 ticks; the dip ran from tick 1
  to 10 after each `KINETIC_HIT` entity event, and only on hits more than 10 game ticks after the last one,
  as `LivingEntity#onKineticHit` gates it. No hit sound plays because the patched component has none, as in
  vanilla.
- Delayed level events: a zombie with Weaving killed 29 blocks away; level event 3018 scheduled
  `block.cobweb.place` 14 ticks out (`distanceDelay`: 29 / 40 * 20) and it played 14 ticks later. The sound log
  (`MCR_SOUND_LOG`) now notes each delayed sound when it is scheduled.
- stop_sound in mod mode: not run. The machine had about 1 GiB of RAM free beside another agent's mod
  benchmark, too little for the Fabric JVM; moved to `vanilla-parity-7`.

#### Typed commands rejected (reported by the batch 1-4 live checks)
- Not a client bug: the chat screen and the autopilot `chat` step send every typed command
  (`ScreenAction::SendCommand`, `Play::submit_chat`); only clicked `run_command` goes through the local
  check. The "Unknown or incomplete command" lines came from the server.
- Cause: `op Bot` on the console before `Bot` had joined. The server resolves an unseen name through the
  profile lookup, and `Bot` is a real account, so ops.json held that account's UUID instead of the offline
  UUID the player joins with. Opping after the join works: `/setblock`, `/summon` and `/time set` all ran.
- Regression test `chat_sends_commands_the_local_parse_rejects` (mcr-ui): Enter sends the command even when
  the local parse shows an error. The autopilot no longer cuts `chat` and `type` lines at `#`, which had
  truncated commands with block tags or colours.

#### Re-audit
- Spot-checked against the decompiled client: `BeaconBlockEntity#tick` (10 blocks a tick, `% 80` recount),
  `KineticWeapon#damageEntities` and `LivingEntity#onKineticHit`, `SystemToast` `pack.copyFailure`,
  `DebugScreenEntries` profiles.
- Leftovers, all in `vanilla-parity-7`: 26 of vanilla's 43 debug entries missing and the F3 text in mcr's
  own order (Q1, Q2); the World Options page doesn't scroll (Q3); no separate cloud depth target with improved
  transparency (Q4); live checks left: stop_sound in mod mode (Q5), local-world rows (Q6), pack drop toast
  (Q7). P6 stays owner-only (Q8).
- Kept as designed: the F3+L zip holds this client's own metrics, and the F3+1 pie has flat sections, because
  there is no nested profiler.
