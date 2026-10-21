## Why

`vanilla-parity-4` closed 175 gaps in 21 batches. Its close-out re-read every batch against the decompiled
26.3 client. Most gaps are fully closed, but the batch reports and the re-audit left 28 smaller
differences. 9 are visible and 19 are minor. None is gameplay-breaking. Two bugs found during the close-out
were fixed there. Block entities in chunk data never reached the scene, and an occlusion regression in
forests did not reproduce; its numbers are in that change's close-out notes.

## What Changes

Each leftover is recorded with vanilla's behaviour, mcr's code location, the impact and a severity
(`design.md`). The fixes are grouped into 8 batches, each sized for one implementation agent
(`tasks.md`):

1. Placement prediction and adventure predicates
2. Command suggestions and chat acknowledgements
3. Riding and projectiles
4. Entity rendering and particles
5. First-person spear feedback and spectator menu order
6. Screens and debug keys
7. Level event and mod-mode sounds
8. World rendering: Fabulous transparency, sampling, beacon cadence, cracks and leaf shading cost

## Out of scope

- The gaps `vanilla-parity-4` already closed, and its deferred items (`projectile_power` is now handled;
  `move_minecart_along_track`, debug renderers, `debug_sample`, the demo popup and game event 13 stay
  deferred).
- Realms, telemetry, the friends list, narrator speech and player reporting.

## Impact

- Affected specs: `gameplay-correctness`, `command-suggestions`, `secure-chat`, `entity-rendering`,
  `sound-events`, `ui-render`, `client-options`, `voxel-rendering`, `block-entity-rendering`.
- Code: `mcr-client`, `mcr-physics`, `mcr-app`, `mcr-ui`, `mcr-entity-model`, `mcr-particles`,
  `mcr-render`, `mcr-block-entity` and `mcr-modbridge`. No protocol changes.
