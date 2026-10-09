## Context

`mcr-nbt` reads disk NBT, `mcr-world` holds `ChunkColumn`, `mcr-data` maps block names and properties to state ids. Vanilla 26.3 stores region files per dimension; recent versions moved dimension data under `dimensions/<namespace>/<path>/`.

## Decisions

- Whole region files are read into memory (`std::fs::read`) rather than mmap: no `unsafe`, and a region is at most a few MB. Chunk decode is parallelised with rayon.
- Palette resolution uses a per-block table of default state plus property strides, so lookup is a name hash plus arithmetic. Property order comes from `state_properties`.
- Biomes are not in `mcr-data` (datapack registry), so ids are interned by name in a shared table owned by `World`; its names are exposed for the renderer. A hard-coded list was rejected as version-specific.
- Block entities are stored as network-NBT bytes (minus `id`, `x`, `y`, `z`), with type ids from the `minecraft:block_entity_type` registry.
- Layout detection prefers `dimensions/` when present, else legacy.
- Every offset and length is validated against the file size before slicing.

## Risks / Trade-offs

- Biome ids are not the server's synced registry ids: fine for offline viewing, must not be mixed with network chunks.
- Reading whole files costs memory for large worlds: regions are loaded and dropped per request.
