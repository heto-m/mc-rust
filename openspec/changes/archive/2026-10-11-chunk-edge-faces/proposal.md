## Why

Renders of a flat ocean show faint stepped light lines along chunk edges at the render-distance border. The padded mesher input treats a missing neighbour column as air with full sky light, so the outermost columns emit water and sand side faces toward chunks that are not loaded. The world viewer only waited for neighbours it had requested; the ones beyond the render distance were never requested, so the edge columns meshed against nothing. Vanilla instead compiles a section only once all eight surrounding chunks exist (`SectionUpdateTracker.hasAllNeighbors`) and re-dirties the 3x3 columns around a chunk when it arrives (`enableChunkLight`).

## What Changes

- A column is meshed only when it and its eight neighbours are in the chunk store; until then its sections stay unrendered.
- Loading or unloading a column dirties every section of the loaded 3x3 around it, including diagonals, so neighbours finish meshing or drop out.
- A column that loses a neighbour is removed from the renderer; mesh results for columns no longer complete are discarded.
- The world viewer loads one extra ring past the render distance, matching the vanilla server, so every column inside the distance can mesh.
- The "pending neighbour" heuristic (`MeshWorld::is_pending`) and the render-distance filter on dirty sections go away; readiness alone decides.

## Out of scope

- Fog reaching full density at the region edge. Water at the edge still shows the void beneath it, as in vanilla.
- Light-readiness tracking per column (vanilla's `lightOnInColumn`). 26.3 chunk packets carry light, so a column's light arrives with it.

## Impact

- Affected specs: `world-viewer` (stream ring, mesh pipeline).
- Code: `mcr-world` store dirtying and `neighbourhood_loaded`; `mcr-app` render system, game streaming, network world.
