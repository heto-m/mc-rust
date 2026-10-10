## Context

`Renderer` already draws terrain in one render pass with a fullscreen analytic sky. Effects are added inside that pass (sky before terrain; clouds, weather and destroy decals after it with depth test) so the app only supplies data. Particles live in their own crate and record through `Overlay`/`OverlayCtx`, so the app chains them before the UI overlay.

## Decisions

- Sky objects use one small pipeline family (position, uv, colour) with a dynamic-offset uniform holding the model matrix and tint; stars are a static buffer generated from vanilla's seed (10842).
- Celestial angles come from the vanilla `day` timeline (cubic-bezier eased 360 degrees per day, noon is 0); moon phase is `day / 24000 mod 8`.
- Clouds: the 12-block cell mesh is rebuilt on the CPU only when the camera crosses a cell, the camera relative position changes or the radius changes; scroll is a uniform offset.
- Weather columns are built per frame on the CPU from a `WeatherSource` trait (heights, precipitation, light), bounded to (2r+1)^2 with r at most 10.
- Particles: structure-of-arrays pool per render type, cap 16384 each, spawn counts from the network are clamped; instance data is uploaded each frame and drawn as instanced quads.
- Sprite sets are read from `particles/*.json`; all particle sprites plus block atlas sub-UVs are supplied by the caller as a texture atlas built by `ParticleAtlas`.

## Risks

Particle behaviour is reimplemented from the reference semantics, not transliterated; unlisted types use a generic fallback.

## App integration

See the crate docs of `mcr_particles` and `mcr_render::effects`.
