# Proposal

## Why

The first sweep covered 28 mods, mostly item lists, tooltips and visuals. Whole groups of popular Fabric client mods
for 26.3 have never run under the bridge: minimaps, HUD overlays, chat mods, camera tools, inventory utilities,
capes, sound mods and more libraries. Three items from the first sweep were left open (EMF/ETF with a real pack,
Mouse Tweaks drag-clicking, Controlling's keybind sync), and a block-entity section was seen culled about once per
frame in the plain entity scene.

## What Changes

- Sweep about 30 more of the most-downloaded Fabric client mods for 26.3 through `mcr --mods` offscreen: each alone,
  then together with the first sweep's set, joining a vanilla server. Extend `docs/mod-compat.md`.
- Re-check the open items: EMF/ETF with a pack that uses their features, Mouse Tweaks drag-clicking through new
  autopilot `down`, `up` and `wheel` steps, and Controlling's keybind sync.
- Find the section culled in the entity scene and fix any false culling in the shared reached-sections path.
- Fix the bridge gaps behind broken or partial mods where the fix is contained, with test-mod regression tests.
- Policy entries for mods that cannot work under design B.

Out of scope: OpenGL renderers and minimaps that call OpenGL directly, terrain-model mods, and mods not released for
Fabric 26.3.

## Impact

- `mcr-app` (autopilot), `mcr-modbridge` (policy data and whatever the fixes need), `java/mcr-bridge`,
  `java/mcr-testmod`, `docs/mod-compat.md`, `docs/mod-bridge.md`.
