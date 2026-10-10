## Context

Entities, block entities and the hand are separate GPU scenes drawn in an overlay after the terrain pass.

## Decisions

- `Overlay` gains an optional `opaque` hook that runs inside the terrain pass between opaque and translucent terrain; opaque entity and block entity parts draw there, blended parts draw in the post pass.
- Entity and flat shaders take fog parameters through `LightInfo` so underwater and render-distance fog match terrain.
- Block entities are pulled from chunk data on load and from `block_entity_data`; removal follows block changes and chunk unload. Light is sampled from the client chunk store.
- Maps: a store keyed by map id keeps a 128x128 RGBA texture, updated with the 4-brightness colour palette; decorations are drawn as sprites on top.
- Passenger offsets come from a per-vehicle table of attachment heights.

## Risks

Drawing inside the terrain pass requires restoring the terrain bind groups afterwards.
