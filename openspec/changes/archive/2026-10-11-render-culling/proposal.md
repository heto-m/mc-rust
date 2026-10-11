## Why

Sections are frustum-, graph- and occlusion-culled, but entities, pickups and block entities use ad-hoc tests: block entities test a 1.8 block sphere and skip the frustum for beacons, flying pickups are never culled, and terrain is only limited by the loaded square.

## What Changes

- Entity and flying-pickup culling share one tested bounding-box frustum plus `shouldRender` distance rule.
- Block entities get per-kind render boxes (beam kinds unbounded in height) and per-kind view distances through pure, tested functions.
- Sections wholly inside the fully fogged part of the render-distance fog are skipped by the GPU cull passes and the translucent sort.
- Hi-Z reuse for entities and block entities is evaluated and rejected, with the reason recorded in the design.

## Capabilities

### New Capabilities
- `render-culling`: frustum and distance culling of entities, pickups, block entities and sections beyond the fog.

### Modified Capabilities

## Impact

`mcr-render` (cull shader, translucent build, `beyond_render_fog`), `mcr-block-entity` (`cull` module), `mcr-app` (entity layer).
