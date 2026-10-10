# Proposal

## Why

`mcr-entity-model` can draw a hand-assembled row of mobs, but the client has no way to turn live server
entities (type, interpolated transform, entity data, equipment) into pictures. The app needs one scene API
that covers every 26.3 entity type with vanilla textures, variants, armor, held items and animation.

## What Changes

- `EntityScene`: `update` takes an iterator of `EntityView` (type, interpolated transform, limb swing, age,
  pose, entity data, equipment, name); `render` writes depth-tested draws into the caller's render pass with
  caller-chosen colour/depth formats and reverse-Z.
- Type mapping for all entity types: model layers, textures, baby variants, variant textures from entity data
  through a caller-supplied registry lookup, scale. Types without a renderer fall back to a hitbox cube.
- Layers: equipment-asset armor with dyed leather and trims, saddles, harnesses, horse/wolf/llama bodies,
  wings, held items, dropped items, item frames, falling blocks and block displays, sheep wool, collars,
  villager outfits, name-tag hook.
- Procedural `setupAnim` ports for the biped, quadruped and several special families plus keyframe
  animations driven by pose and state; hurt tint and death rotation.
- Players: vanilla default skin choice by UUID, slim or wide arms, jacket/sleeve/pants overlay parts.
- Renderer rework: batches per (pipeline, model, texture array), per-instance bone data in storage
  buffers, dynamic texture arrays.
- Extractor additions where the existing dump lacks data.

Out of scope: shadows, fire overlay, particles, boss bars, rendering of the first-person hand, glint.

## Capabilities

### New Capabilities
- `entity-scene`: in-world entity scene API, type mapping, layers, animation, players and name tags.

### Modified Capabilities
- `entity-models`: instanced rendering requirement now covers pipelines, dynamic texture arrays and lighting.

## Impact

`crates/mcr-entity-model` (new deps on `mcr-protocol`, `mcr-assets`, `rustc-hash`, `uuid`), `tools/extractor`.
No edits to other crates.
