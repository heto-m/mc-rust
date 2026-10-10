# Design

## Context

`mcr-entity-model` turns an `EntityView` into a `Plan` (parts, rig, attachments) and then into GPU instances. Rigs write procedural bone poses onto a model's rest pose by semantic `Slot`; keyframe animations cover some mobs; held items attach to a bone with a fixed matrix. See proposal.md for motivation.

## Goals / Non-Goals

**Goals:** vanilla-exact hand matrices, `setupAnim` parity for the listed mobs, sprite and effect entities, shadows and glint with no new per-entity allocation, update of 2000 entities under 1 ms.

**Non-Goals:** a general scripting layer for animation, shadows from lights other than the vertical drop.

## Decisions

- Hand attachments carry a per-family arm offset (position delta applied to the arm pose before its matrix is built, like vanilla's `part.x += offset`) instead of a post-multiplied translation, so offsets rotate with the arm exactly as in vanilla.
- Rigs stay a closed `Rig` enum plus `Slot` bone names resolved once per model; mob-specific inputs (squish, tail angle, puff state, carried block) travel in `AnimIn` so rigs remain pure functions of data.
- Billboards are flat quad models oriented in `update` from a camera hint set on the scene; shadows are one flat quad per entity at the heights returned by the callback.
- Glint reuses the Additive pipeline with a scrolling `uv_offset` on the glint texture; the glint pass draws the same model again so no geometry change is needed.
- Performance: scratch buffers are reused, texture and model lookups are cached, and pose evaluation runs in parallel only above an entity-count threshold.

## Risks / Trade-offs

- [Parallel pose evaluation adds sync cost for small scenes] -> threshold on entity count, measured with the criterion bench.
- [Shadow callback cost per entity] -> at most four samples per entity and early-out beyond 16 blocks of fall.
