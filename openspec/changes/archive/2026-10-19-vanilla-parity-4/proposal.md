## Why

The owner expects vanilla still has many unfound gaps. A systematic read-only audit compared the native
client with the decompiled 26.3 client across physics, interaction, containers, HUD, entity and world
rendering, sound, chat, options and every clientbound packet. It confirmed 175 gaps: 17 gameplay-breaking,
94 visible and 64 minor. Several features exist but are never wired: the game view never provides
merchant offers, stonecutter results or tooltips; mount screens close at once; level events other than
block breaks and jukeboxes are dropped; effects never reach the lightmap or fog. A player in bed cannot
get up. Invisible entities are fully drawn. Spears do not attack. Adventure players cannot use `can_break`
tools.

## What Changes

The change records each gap with vanilla's behaviour, mcr's code location, the player-visible impact and
a severity (`design.md`). It then groups the fixes into 21 batches, each sized for one implementation agent,
highest impact first (`tasks.md`):

1. Containers that do not work (mount, merchant, stonecutter, loom, anvil, crafter)
2. Sleeping and respawn state
3. Use and attack loop (spears, release, per-tick mining)
4. Targeting, adventure mode and placement
5. Physics collisions (entities, world border, push, push-out)
6. Invisible entities and entity render state
7. Effect vision and fog
8. Riding and vehicles
9. Spectator menu and actions
10. Level events and client-side sounds
11. Item tooltips, bundles and screen extras
12. HUD
13. Camera effects and screen overlays
14. Movement effects and equipment
15. First-person hand
16. Entity motion and projectiles
17. Particles
18. Entity layers, name tags, maps and compasses
19. Chat and commands
20. Options, keys and screens
21. Block rendering fidelity

## Out of scope

- Mod-mode parity (the Java GUI and Java-owned sounds already cover some of these gaps there).
- Items deferred in `parity-followups-1` (`projectile_power`, experimental minecart lerp, debug
  renderers, demo popup).
- Realms, telemetry, narrator speech and player reporting.

## Impact

- Affected specs: `player-physics`, `gameplay-correctness`, `ui-container-screens`, `ui-render`,
  `entity-rendering`, `sky-rendering`, `voxel-rendering`, `sound-events`, `command-suggestions`,
  `client-options`.
- Code: mostly `mcr-app`, `mcr-client`, `mcr-physics`, `mcr-ui`, `mcr-entity-model`, `mcr-render`,
  `mcr-mesh`, `mcr-particles` and `mcr-block-entity`. No protocol changes. All packets the fixes need are
  already encoded.
