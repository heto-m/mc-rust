# Proposal

## Why

Chests, signs, banners, heads, shulker boxes and similar blocks have empty or partial block models in 26.3 and are drawn entirely by block entity renderers. Without them the world shows holes (chests, shulkers, banners, heads, beacon beams, portals) and unreadable signs.

## What Changes

- New crate `mcr-block-entity` with strictly bounded parsing of network NBT (chunk packet block entities and `block_entity_data`) into typed per-kind state.
- A `BlockEntityScene` that turns `(pos, block state, kind, data)` entries plus light, game time and `block_event` animation state into instanced draws on the entity-model GPU renderer, with reverse-Z depth and the camera-relative origin convention.
- Renderers for chest family (single, double, trapped, ender, copper, christmas) with lid animation, shulker box, bell, banner with pattern layers, all skull types including dragon and player heads, lectern and enchanting table books, decorated pot with sherds, conduit, beacon and end gateway beams, end portal and gateway shader, campfire items, spawner, trial spawner and vault displays, brushable block item, shelf items, copper golem statue and moving piston blocks.
- World-space sign and hanging sign text with vanilla font metrics, dye colours, glowing outline, both sides, and the vanilla outline distance cut.
- Out of scope: beds and sign bodies (plain block models in 26.3, meshed by terrain), structure block bounding boxes, test instance blocks, crumbling overlay, wiring into the app.

## Capabilities

### New Capabilities
- `block-entity-data`: bounded NBT parsing of block entity state into typed kinds.
- `block-entity-rendering`: drawing block entities in a caller's render pass, including animations and sign text.

### Modified Capabilities

## Impact

New crate `crates/mcr-block-entity`; small additive getters in `mcr-ui`. Depends on `mcr-entity-model`, `mcr-ui`, `mcr-nbt`, `mcr-protocol`, `mcr-data`, `mcr-assets`.
