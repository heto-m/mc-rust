# Design

## Decisions

- **Static tables.** `PhysicsData` flattens `mcr-data` into per-state records (shape slice into an `f64` box array, flags, friction, speed/jump factor, fluid bits, climb/stuck/bounce classes). Classification is by block name once at startup; there is no per-tick string work.
- **World trait.** `CollisionWorld::block_state(x, y, z) -> u32` plus defaulted `is_loaded` and `min_y`. Unloaded columns must report air; `is_loaded` only drives the gravity fallback vanilla applies in unloaded chunks.
- **Shapes as box lists.** Vanilla's `VoxelShape.collide` works on a merged grid; for non-penetrating entities this equals per-box AABB collision with the same 1.0E-7 rules. Penetration inside a multi-box shape is the only known divergence.
- **Gather once.** `move` gathers colliding block shapes for the movement hull plus step height into a stack buffer, then reuses it for the base collision, step-up candidates and the supporting-block search, filtering by exact intersection where vanilla does (candidate heights depend on which shapes touch the step-up box).
- **Single/double precision.** Attribute math in `f64` then cast to `f32` exactly where `getSpeed`, jump power, friction and `moveRelative` do; the sine table is the vanilla 65536-entry `f32` table.
- **Tick order.** `baseTick` fluid update and swimming, `aiStep` (sprint/crouch/flight logic, input to impulse, jump, travel), block-inside effects, then pose update, matching `LocalPlayer`.
- **Packets.** `MovementOutput` carries the chosen move kind and flags; encoding stays in `mcr-protocol`.

## Approximations

- Block-inside effects use the final bounding box instead of the swept path.
- Fluid flow vectors follow vanilla's neighbour-height algorithm using fluid bits from data.
- No world border, entities, elytra, vehicles; scaffolding is a climbable with its static shape.
