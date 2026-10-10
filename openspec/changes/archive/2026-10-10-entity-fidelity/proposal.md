# Proposal

## Why

The entity scene draws every vanilla type, but held items sit slightly off the hand, many mobs only turn their head instead of running their `setupAnim`, projectile and effect entities are missing or drawn as spinning 3D items, and there are no shadows or enchantment glint. 2000 mixed entities also cost about 1.5 ms of CPU per frame, above the 1 ms budget.

## What Changes

- Held items follow vanilla `translateToHand` per model family (skeleton and slim-arm offsets, illager, allay, vex, copper golem, baby offsets) followed by the item's third-person display transform, with bow, crossbow, trident and spear arm poses.
- Per-mob procedural rigs for fish, squid, bat, blaze, ghast, slime, wolf, enderman, golems, guardian, phantom, dragon, wither, horses, llama, strider, hoglin, ravager, shulker, vex and parrot, plus the extra layers (snow golem pumpkin, mooshroom mushrooms, carried block, golem flower, held mouth/hand items).
- Camera-facing sprite entities: thrown items, experience orbs, fishing bobber and line, firework rocket; end crystal with beam hook; lightning bolt geometry.
- Blob shadows projected on the block surface below via a caller-supplied block-height callback, and scrolling additive enchantment glint on armor and held items.
- Faster `update`: reused scratch buffers, no per-frame allocation, parallel pose evaluation when it pays.
- Out of scope: first-person hands, name tag rendering, entity picking, shadows on fluids.

## Capabilities

### New Capabilities

### Modified Capabilities
- `entity-scene`: held-item placement, per-mob animation coverage, projectile and effect entities, shadows, enchantment glint and the CPU budget become normative.

## Impact

Only `crates/mcr-entity-model` changes. `EntityScene` gains an optional shadow-height callback and a shadow toggle; existing calls keep working. A criterion bench pins the 2000-entity update time.
