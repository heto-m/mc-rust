## Why

The singleplayer world-select screen needs to list vanilla saves, and the renderer needs real terrain to test against before networking is finished. Both require reading the on-disk Anvil format.

## What Changes

- New crate `mcr-anvil`: region file reader (all vanilla compression types, external `.mcc` chunks), chunk NBT to `ChunkColumn` conversion, `level.dat` summaries, and a `World` handle with parallel chunk loading.
- Supports both the legacy dimension layout (`region/`, `DIM-1/`, `DIM1/`) and the `dimensions/<namespace>/<path>/` layout.
- `inspect` example printing a world summary and block histogram.

## Capabilities

### New Capabilities
- `anvil-reader`: reading vanilla world saves from disk.

### Modified Capabilities

## Impact

New workspace member `crates/mcr-anvil`; adds `lz4_flex` and `rayon` dependencies. Depends on `mcr-nbt`, `mcr-world`, `mcr-data`.
