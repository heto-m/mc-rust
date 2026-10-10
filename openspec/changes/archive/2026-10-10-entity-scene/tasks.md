# Tasks

## 1. Renderer core

- [x] 1.1 Rework the GPU renderer: dynamic size-class texture arrays, three pipelines, lighting, tint and hurt overlay, reverse-Z; verify offscreen PNG of the original mob row.
- [x] 1.2 Scene types (`EntityView`, registries, lights) and batching; verify unit test of sort and counts.

## 2. Type mapping

- [x] 2.1 Static type table for every extracted renderer with baby layers; verify test that every entity type resolves.
- [x] 2.2 Variant resolution from entity data (cat, wolf, frog, pig, cow, chicken, rabbit, horse, llama, parrot, axolotl, fox, panda, mooshroom, villager, sheep, fish, painting); verify per-type unit tests.
- [x] 2.3 Hitbox fallback for unmapped types; verify test.

## 3. Animation

- [x] 3.1 Biped, quadruped, chicken, villager, creeper, spider, slime rigs; verify unit tests and screenshots.
- [x] 3.2 Keyframe animations from pose/state; hurt tint and death rotation.

## 4. Layers

- [x] 4.1 Equipment assets: armor, leather dye, trims, saddles, horse/wolf/llama bodies, wings; verify trimmed armor screenshot.
- [x] 4.2 Held items, item entities, frames, falling blocks, block display, paintings.
- [x] 4.3 Players: default skins by UUID, slim/wide, overlays; name-tag hook.

## 5. Verification

- [x] 5.1 Lineup screenshots of 30+ types and variants checked by eye.
- [x] 5.2 2000-entity preparation benchmark.
- [x] 5.3 fmt, clippy, test, archive.
