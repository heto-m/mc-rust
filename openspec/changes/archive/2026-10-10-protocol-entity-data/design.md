# Design

## Context

Registry-dependent values (block states, variants, items) are plain integer ids on the wire and the client resolves them later. `ItemStack`, `Component` and the profile types are owned by other changes.

## Goals / Non-Goals

**Goals:**
- Lossless decode/encode of every particle type and entity data serializer in 26.3.
- Typed enums for small fixed sets (pose, direction, arm, dye colour, states).

**Non-Goals:**
- Resolving ids to names, non-empty data component patches in item particles.

## Decisions

- `Particle` is an enum with a `Simple(id)` fallback; typed variants carry options and a small kind enum where several particle ids share one options shape (block-state, spell, colour, geyser).
- `EntityDataValue` has one variant per serializer, discriminant equal to registration order; the order is verified by a test and the table lives in one macro.
- Holder-typed variants store `i32` registry ids; painting variant uses `IdOr<PaintingVariant>` with its inline fields.
- Item particles use a local item template (item, count, patch) since its field order differs from `ItemStack`; only the empty patch decodes until the component patch codec exists, other patches are a decode error.
- Resolvable profiles are defined next to the entity data because no shared profile type exists yet.
- Out-of-range enum ids are decode errors instead of vanilla's silent clamping.

## Risks / Trade-offs

- A non-empty item particle patch fails the enclosing packet until the patch codec lands.
