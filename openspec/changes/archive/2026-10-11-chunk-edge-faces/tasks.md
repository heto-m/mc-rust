## 1. Store

- [x] 1.1 Dirty the loaded 3x3 (diagonals included) on column insert and remove
- [x] 1.2 Add `ChunkStore::neighbourhood_loaded`

## 2. Render pipeline

- [x] 2.1 Queue dirty sections through `queue_dirty`, dropping and unrendering columns without all eight neighbours
- [x] 2.2 Gate submit, urgent meshing and upload on the loaded neighbourhood
- [x] 2.3 Remove `MeshWorld::is_pending` and the render-distance filter on dirty sections

## 3. World viewer

- [x] 3.1 Request one ring past the render distance

## 4. Tests and verification

- [x] 4.1 Store: diagonal dirtying on load/unload, neighbourhood check
- [x] 4.2 Render system: unready column not queued, last diagonal queues it, neighbour unload unrenders, border column meshes when its outer ring arrives
- [x] 4.3 Game: every in-range column has its neighbours requested, within the unload margin
- [x] 4.4 Flat-ocean screenshot before/after: edge light lines gone; terrain render unchanged inside the disc
