# Proposal

## Why

The app draws terrain, entities and held items but leaves block entities, maps, several hand poses and correct blending of entities with translucent terrain unwired. Several screens also deviate from vanilla (death screen lifetime, title scale, passenger seats).

## What Changes

- Block entities from chunks and `block_entity_data` render through `BlockEntityScene`, with lids, bells, beacon beams, conduit and skull skins.
- `map_item_data` feeds a 128x128 map texture store; maps render in hand, in item frames and as GUI items.
- Shield, spyglass, trident riptide and totem hand states.
- Entities and block entities draw opaque before translucent terrain and blended parts after, with fog.
- Death screen closes on respawn or healing, passenger seats per vehicle type, explosion emitter particles, brightness option, titles at vanilla scale.

## Capabilities

### New Capabilities
- `app-polish`: how the app wires block entities, maps and hand items and orders translucent rendering.

## Impact

`crates/mcr-app`, additive changes in `mcr-entity-model`, `mcr-block-entity`, `mcr-render`, `mcr-ui`, `mcr-particles`.
