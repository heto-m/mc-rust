## Why

The packet audit and render-polish-1 left a list of vanilla behaviours that were partial or missing. This change closes the ones a player sees, and records the rest with a reason.

## What Changes

- Code of conduct: the configuration packet shows a scrollable screen (acknowledge or disconnect), and "do not notify again" remembers the text hash in `servers.dat`.
- Spectator camera: `set_camera` follows another entity, suppresses movement and shows its view.
- Locator bar: tracked waypoints draw on a bar that replaces the experience bar, with styles from `waypoint_style/*.json`.
- Tab list: player heads from the `textures` profile property (downloaded off-thread from `textures.minecraft.net`), default skins otherwise; the same skins now reach world players and the first-person arm.
- Leash ropes, elder guardian curse ghost, transient blocks, server-sent icons, custom chat completions, boss bar darken and fog flags, head yaw lerp.
- Render follow-ups: trident glint, cushion, ominous item spawner, display billboard modes, ender dragon lean. End crystal base height verified against vanilla's pose math, no change needed.
- Debug overlay frame, tick and ping charts.
- Deferred with reasons: see design.

## Impact

- Affected specs: `client-connection`, `entity-rendering`, `ui-menu-screens`, `playable-client`.
- Code: `mcr-client` (conduct link, locator, camera entity, completions, transient blocks, flight history), `mcr-auth` (textures property), `mcr-ui` (conduct screen, debug charts), `mcr-entity-model` (leash, ghost, cushion, spawner, billboards, lean, glint), `mcr-app` (skins, hud, play, shell, servers).
