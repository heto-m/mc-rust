## Why

`vanilla-parity-5` closed 28 gaps in 8 batches. Its design notes record a few differences that were
knowingly left: the unfinished parts of Fabulous transparency, the skin elytra texture, a leaf-shading
case and a beacon cadence detail, plus two items only checked offscreen. None is gameplay-breaking.

## What Changes

Each leftover is recorded in `design.md` with vanilla's behaviour, mcr's code location, impact and a
severity. The fixes are grouped into 3 small batches (`tasks.md`):

1. Fabulous transparency completion
2. Skin elytra texture and live checks
3. Terrain late-phase cost and beacon beam cadence

9 items: 3 visible, 6 minor, 0 gameplay-breaking.

## Out of scope

- L25 texture filtering NONE at mip seams, declined for GPU cost (see `design.md`, "Declined").
- Parity-4 deferred items (`move_minecart_along_track`, debug renderers, `debug_sample`, demo popup, game
  event 13), Realms, telemetry, friends list, narrator speech and player reporting.

## Impact

- Affected specs: `voxel-rendering`, `entity-rendering`, `block-entity-rendering`.
- Code: `mcr-render`, `mcr-app`, `mcr-entity-model`, `mcr-auth`. No protocol changes.
