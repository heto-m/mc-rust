## Why

The client needs a compact, renderer- and physics-friendly in-memory world that decodes the 26.3 chunk wire format without per-block allocation.

## What Changes

- New crate `mcr-world`: block/biome ids, paletted containers, chunk sections, columns, and a chunk store with a dirty-section set.
- Decoders for the chunk data and light data structs of the level-chunk-with-light packet (envelope excluded).
- Criterion benches for get/set/unpack and full-column decode.

## Capabilities

### New Capabilities
- `world-storage`: wire-compatible paletted containers, chunk columns, light, heightmaps and the chunk store.

### Modified Capabilities

None.

## Impact

New crate `crates/mcr-world` depending on `mcr-buf`; adds `criterion` to workspace dependencies.

## Out of scope

Packet envelope, NBT parsing (block entity data is kept as raw bytes), light propagation, meshing, persistence.
