## Context

`MeshInput::snapshot` copies a one-block border from neighbour columns. A missing column leaves that border as air (state 0) with `UNLIT_LIGHT`, so faces toward it are emitted fully lit. `RenderSystem::submit` gated meshing on `MeshWorld::is_pending`, which only covered chunks that had been requested. Chunks past the request disc, and chunks the save lacks, counted as settled. The store also dirtied only the four cardinal neighbours on insert, so a column waiting on a diagonal neighbour would never have been re-meshed.

## Decisions

- **Readiness = 3x3 loaded.** `ChunkStore::neighbourhood_loaded(pos)` does nine hash lookups. It is checked when dirty sections are queued, when a queued column is submitted, before urgent (block-change) meshing, and before uploading a finished mesh.
- **Event-driven, no scans.** Insert and remove dirty the loaded 3x3, so a column's readiness can only change in a frame in which it is dirty. `queue_dirty` checks only the columns touched by that frame's dirty drain. Unready columns are dropped from the queue and their sections removed from the renderer. The neighbour's arrival re-queues them. No blocked set is kept and no per-frame pass covers all columns.
- **Light and biome updates still dirty only cardinal neighbours.** Widening them would raise remesh work for every server light packet. Insert and remove need the diagonals for correctness, and those two are what changes readiness.
- **Already-compiled sections whose neighbour unloads are unrendered**, not kept. Vanilla keeps the old mesh but never rebuilds it without neighbours. Dropping it keeps the rule simple: a section is drawn only while its 3x3 exists, and a later rebuild can never mesh against a missing column.
- **Viewer border ring.** `Game` requests a column when it, or any column bordering it, is within the render distance. That is the ring the vanilla server sends past the view distance. The unload margin (distance + 2) still contains the ring, so columns do not thrash.

## Risks

- Holes in a save (chunks never generated) leave the bordering columns unrendered, like the region edge.
- Unloading now hides a neighbour ring one frame earlier than before. That ring sits at the far edge and is out of range anyway.
