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
