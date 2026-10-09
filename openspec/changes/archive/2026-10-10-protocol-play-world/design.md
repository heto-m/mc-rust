# Design

## Context

Packets live in `play/clientbound/world.rs` and `entity.rs` (owned by this change) on top of `mcr-buf` derives. Registry-backed ids (entity types, blocks, attributes, effects) stay plain VarInts; resolution happens above the protocol crate. Placeholder shared types (`Component`, `ItemStack`, `Particle`, `SoundEvent`, `EntityDataValues`) are used as field types.

## Decisions

- **Deferred payloads**: `LevelChunkWithLight` is `{x: i32, z: i32, payload: RestBytes}`. Chunk data and light data are back to back with no length between them, so the first decoder (`ChunkData::decode`) leaves the cursor at the light data. Splitting eagerly would need heightmap and NBT parsing in the protocol crate for no benefit.
- **Light bit sets**: vanilla's light masks are byte-array bit sets (VarInt byte count), not the long-array `BitSet` in `mcr-buf`; since light stays opaque this only matters for `mcr-world`, which already uses the byte form.
- **Holder fields**: damage types, mob effects, attributes and world clocks are VarInt registry ids. Sounds use `types::SoundEvent` as the whole holder codec.
- **Equipment**: slots use the enum ordinal with a continuation bit, items use `ItemStack` as the optional-stack codec.
- **Movement**: `VecDelta` is an enum (linear or stepped); the step count lives in the properties VarInt and is derived on encode. `LpVec3` is a helper newtype.
- **Debug values**: an enum with `VillageSections`, `NeighborUpdates`, `Raids` decoded, `Opaque` for other known kinds (payload is the rest of the packet), and a typed `InvalidValue` error beyond the registry size. The value is last in every debug packet so the remainder is unambiguous.
- **Tests**: macro-generated roundtrips; packets with placeholder types are `#[ignore]`d. Byte fixtures are hand-derived.

## Risks / Trade-offs

- [Placeholder codecs may differ from the assumptions above] → documented in the report; owners can swap field types without wire changes elsewhere.
- [Opaque debug kinds] → callers can decode them later with the registry id; nothing is mis-parsed.
