# Design

## Context

26.3 renders beds and sign bodies as block models; chest, shulker, banner, skull, bell, pot, conduit and portals have empty block models. `mcr-entity-model` already provides an instanced skinned renderer (`EntityRenderer`), model layers from `entity_models.json`, size-class texture arrays and an `EntityScene` that can draw `item_display`, `block_display` and mob entities.

## Decisions

- `BlockEntityScene` owns an `EntityRenderer` for model-based kinds and an `EntityScene` for item, block and mob displays (campfire food, shelf, vault, brushable, piston, spawner mob). Both draw in the caller's pass, so the depth and camera conventions are inherited.
- Per entry, the scene evaluates the layer's bone chain on the CPU with the animation overrides (lid angle, bell swing, page flip, flag wave) and writes world matrices; entries are skipped beyond a per-kind view distance.
- Banner pattern layers use the decal pipeline (equal-depth alpha blend) with the dye colour as tint, in emission order.
- Sign text is laid out once per text change through `mcr-ui` into local glyph quads (font pixel space), then transformed per frame into world space and drawn by a small dedicated pipeline sampling copies of the glyph atlas pages. Glowing text adds eight outline copies and is fullbright.
- Portal and gateway use a dedicated pipeline implementing the layered `textureProj` shader on cube faces; beam geometry is generated per section and scrolled by `uv_offset`.
- Light is an input per entry (block and sky nibbles); the scene never reads the world.
- Parsing is separate from rendering and enforces limits on strings, lists and depths before any allocation grows from server data.

## Risks

Atlas pages are shared with the UI renderer; the scene copies page pixels itself and tracks an atlas revision counter (additive `mcr-ui` getter) instead of consuming dirty flags.
