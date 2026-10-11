## Why

Owner playtest round 3: taming a wolf showed no smoke or hearts and the tamed wolf wore no collar. Most of vanilla's per-entity `handleEntityEvent` effects were missing: only the totem and death events were handled. Other reports from the same session: elytra gliding has no pose, elytra breaks creative flight, thrown items render like dropped items, primed TNT neither flashes nor swells, far kelp blinks, the sea floor looks too detailed from above, and the new item-drop prediction has no test.

## What Changes

- Entity events: every client-side effect of vanilla's `handleEntityEvent` overrides (particles, local sounds, animation clocks, state changes), dispatched along the vanilla class chain. Firework explosions get vanilla's starter, coloured sparks with fade, trail and twinkle, and the burst flash.
- Tamed pets: a collar that is still the default red is never synced, so a missing collar colour now means red, for wolves and cats.
- Elytra: glide pose on players and the wings layer; creative flight toggling with an elytra equipped.
- Thrown items: camera-facing billboards like vanilla's `ThrownItemRenderer`.
- Primed TNT and TNT minecarts: fuse swell and white flash, minecart display blocks.
- Kelp: cutout mipmaps keep their coverage like vanilla's `MipmapGenerator`.
- Sea floor: sky light of sections without light data is read like vanilla.
- Item-drop prediction tests; screenshot frame sequences for diffing.

## Impact

- Affected specs: `entity-rendering`, `particle-engine`, `client-state`, `player-physics`, `texture-arrays`, `world-storage`, `world-viewer`.
- Code: `mcr-physics` (creative flight), `mcr-assets` (mip coverage), `mcr-world` and `mcr-mesh` (sky light lookup), `mcr-client` (entity event state, TNT fuses, drop prediction, headless `entities` step and sound log), `mcr-particles` (firework sparks, flash), `mcr-entity-model` (event animation inputs, collars, fangs rig), `mcr-app` (entity event effects and clocks, firework starter, sniffer digging sound).
