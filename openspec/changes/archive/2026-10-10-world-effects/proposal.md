## Why

The world is drawn with a flat sky colour and no atmosphere, weather, particles or block-breaking feedback. A vanilla client needs the sky dome, celestial bodies, clouds, precipitation, particles and destroy-stage decals to look and feel right.

## What Changes

- Extend `Environment` with celestial angles, moon phase, star brightness, sunrise colour, cloud colour/height and dimension skybox; weather modifiers follow the vanilla rain/thunder attribute layers.
- `mcr-render` draws the sky (sun, moon phases, seeded stars, sunrise fan, dark disc, end sky), fancy 3D clouds from `clouds.png`, rain/snow columns from a heightmap/precipitation source, and block destruction decals.
- New crate `mcr-particles`: CPU simulation with a hard cap per render type, GPU-instanced billboards, vanilla sprite sets from `particles/*.json`, per-type behaviour, `level_particles` and block break/crack emitters.
- Lightning flash hook on the renderer.

## Capabilities

### New Capabilities
- `sky-rendering`: sky dome, celestial bodies, stars, end sky, clouds, environment inputs.
- `weather-rendering`: precipitation columns, fog/darkening, lightning flash.
- `particle-engine`: simulation, rendering, emitters and caps.
- `block-destruction-overlay`: destroy-stage decals.

### Modified Capabilities

## Impact

`crates/mcr-render` (environment, new effects modules and shaders), new `crates/mcr-particles`. The app wires the new setters and calls documented in `design.md`; no protocol changes.
