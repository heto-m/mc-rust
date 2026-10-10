## Context

`mcr-anvil` loads columns, `mcr-world::ChunkStore` tracks dirty sections, `mcr-mesh::MeshPool` meshes snapshots, `mcr-render::Renderer` uploads sections. `mcr-app` only showed synthetic terrain.

## Goals / Non-Goals

- Goals: a reusable `Game` / `RenderSystem` / `WorldSource` split; streaming from disk; measurable load times.
- Non-Goals: player physics, UI, networking, light propagation (stored light from disk is used).

## Decisions

- `WorldSource` is a poll-style trait (`request`, `cancel`, `poll`) so a network source can feed the same loop.
- `AnvilSource` loads single chunks on a fixed thread pool, nearest first via a priority queue.
- Biome ids are interned lazily by the reader; colours are built from the biome names known at pool creation and the pool is rebuilt (all columns re-dirtied) when new names appear.
- Uploads are budgeted by wall time per frame, not count, so frame pacing is stable.
- Screenshots use the offscreen renderer and wait until the source is idle and the mesh and upload queues are empty.

## Risks / Trade-offs

- Rebuilding the mesh pool on new biomes causes remeshing bursts early in a session; it only happens when new biome names appear.

## Reader fix

Real 26.3 saves store palette entries as `{id, properties}` or `{"": name}`; `mcr-anvil` only read `Name`/`Properties`, turning most blocks into air. It now reads all layouts, and `World` exposes `open_region`/`load_chunk_in` so callers can cache region files.
