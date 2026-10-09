# Proposal

## Why

Entities (mobs, armor stands, players) need vanilla-accurate models, textures and animation. Vanilla builds
these in client code, so they can only be obtained by running the client's own model definitions.

## What Changes

- Extend the dev extractor with a client-side pass that dumps every baked model layer (part trees, cubes,
  UV offsets, deformation, mirror, texture size), every keyframe animation, and per entity type the
  statically derivable renderer info (textures, layers, shadow radius, scale).
- New crate `mcr-entity-model`: loads those JSON files, unwraps cubes into skinned vertices with vanilla
  UV layout, evaluates bone poses and keyframe animations, and provides hand-written procedural animation
  for biped and quadruped walk cycles and head look.
- GPU side in the same crate: instanced skinned rendering with one draw per model, bone matrices in a
  storage buffer, entity textures in a texture array; offscreen render test.

## Capabilities

### New Capabilities
- `entity-models`: extraction, representation, posing, animation and instanced rendering of entity models.

### Modified Capabilities

## Impact

- `tools/extractor` (new client entry point, renderer scan script), new crate `crates/mcr-entity-model`
  (depends on `wgpu`, `glam`, `bytemuck`, `serde_json`, `png`, `zip`). No changes to existing crates.
