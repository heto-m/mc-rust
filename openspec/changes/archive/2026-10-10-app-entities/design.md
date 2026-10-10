## Context

`EntityScene` owns GPU buffers per prepare, so the world pass and the hand/inventory pass use separate scenes sharing `SceneAssets`.

## Decisions

- Per-entity `Track` in the app records previous/current tick state (position, yaw, head, walk animation, swing, hurt) updated on each 20 Hz tick; views are built per frame with the partial tick.
- Entities draw inside the overlay hook after the world pass (depth loaded, reverse-Z). The hand scene draws after clearing depth with its own projection.
- Name tags are drawn in 2D from `EntityScene::name_tags`; occluded tags are dimmed via a block raycast.
- First-person transforms live in a new `hand` module of `mcr-entity-model` (additive) because geometry caches are private.
