# Proposal

## Why

The app draws the world but no entities, hand or effects. The entity scene and the world-effects
renderer exist as libraries; the playable client must drive them from live network state.

## What Changes

- Build an `EntityScene` from `mcr-client` entity state each frame with render-frame interpolation, limb swing, body/head yaw, synced-registry variants, frustum and distance culling.
- Name tags through `mcr-ui` with vanilla visibility rules.
- First-person hand: arm, held items, equip/swing/use animations.
- Local player model in the inventory screen following the mouse.
- Wire world effects: sky/clouds/weather environment per dimension, destroy decals, particle engine.
- Out of scope: map item data rendering beyond a blank map, translucent-depth ordering of entities behind water.

## Capabilities

### New Capabilities
- `entity-rendering`: app-level entity drawing, name tags, first-person hand, inventory preview and effect wiring.

### Modified Capabilities

## Impact

`crates/mcr-app`; small additive API in `mcr-entity-model` (first-person item contexts, hand emitters).
