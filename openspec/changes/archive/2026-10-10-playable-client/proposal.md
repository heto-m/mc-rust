# Proposal

## Why

The headless client, renderer, UI, audio and settings crates exist but nothing joins them: there is no way to connect to a server and play. This is milestone 1's integration step.

## What Changes

- `mcr --connect host[:port] --name <name> [--game-dir dir]` runs the real client: a network world fed from `mcr_client` state drives the mesher and renderer; block updates remesh with priority.
- Fixed 20 TPS `Client::tick` on the main loop with interpolated camera, vanilla mouse sensitivity and FOV (incl. sprint change), pose-dependent eye height, third-person views.
- Input through `mcr-settings` key bindings from `options.txt`: movement, hotbar, drop, offhand, digging, use, pick block, chat, F1/F3/F5, Tab list, pause and cursor grab.
- Block and entity targeting, black block outline and crosshair.
- HUD (hotbar, items, vitals, xp, chat, titles, boss bars, F3, tab list, sidebar), a minimal replaceable chat input, audio for server sounds and block break/place/step.
- Scripted `--autopilot` and `--screenshot ... --after-ticks N` for headless verification against a live server.
- `mcr-render` gains an additive overlay hook (draw after the world pass with depth available).
- World source role generalised: the render system meshes from any `MeshWorld`; the anvil game and the network world both implement it.

Out of scope: inventory/container screens, entity rendering, particles, weather, sky/clouds, pause menu screens, online authentication.

## Capabilities

### New Capabilities
- `playable-client`: connected play session, input, targeting, HUD, chat input, audio, autopilot.

### Modified Capabilities
- `world-viewer`: meshing is fed by a generic world view; command line gains connect mode.

## Impact

`crates/mcr-app` (most), small additive changes in `mcr-render` (overlay hook) and `mcr-client` (instant break). New app dependencies on `mcr-client`, `mcr-ui`, `mcr-audio`, `mcr-settings`, `mcr-physics`.
