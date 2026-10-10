# Design

## Context

`mcr-entity-model` has the model/pose/animation/GPU primitives (see `entity-models`). `mcr-client` exposes
entities as `Entity` (type id, interpolated position/yaw/pitch, `FxHashMap<u8, EntityDataValue>`, equipment
stacks). The renderer frame is camera-relative with a rotation-only reverse-Z view-projection.

## Decisions

- **Input type.** `EntityView` borrows protocol types (`EntityDataValue`, `ItemStack`) so the app converts
  with field copies. Data indices are resolved per entity class inside the crate, not by the caller.
  Registry-backed ids go through a `Registries` trait the caller implements.
- **Parts.** An entity resolves to a list of parts (model, texture key, pipeline, tint, pose source). Layers
  (armor, wool, saddle) are extra parts that share the base part's pose; held items attach to bones of the
  base pose. Resolution is a pure function from view to parts, so it is unit-testable without a GPU.
- **Textures.** Size-class texture arrays (64/128/256/512) grown by doubling, keyed by resource path or
  synthetic key (skins, trims, dyed armor). Textures load lazily from `ResourceManager` so resource packs apply.
- **Draw batching.** Instances sort by (pipeline, class, model); one `draw_indexed` each. Bones are
  pre-multiplied with the instance transform on the CPU into one storage buffer.
- **Held items and blocks.** Item and block models bake to static `Model`s with one part per sprite; display
  transforms come from `mcr-assets`.
- **Animation.** Hand-written rigs per family operate on bone names; keyframe animations bind per model and are
  selected from pose/state tables. Unknown bones are skipped so missing rigs degrade to the rest pose.
- **Hostile input.** Entity data of unexpected type or index is ignored; variant ids out of range select
  defaults; counts (stack clones, passengers) are clamped.

## Risks

- Vanilla renderer logic is large; unported behaviors fall back to the rest pose and are listed in the type table.
- Lazy texture loading may hitch; textures are decoded once and cached.
