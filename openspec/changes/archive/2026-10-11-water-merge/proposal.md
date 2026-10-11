## Why

A flat ocean or lake emits one translucent quad per surface block, front and back, so a 16x16 water section costs 512 quads. That is vertex throughput and draw work for geometry that is visually one plane.

## What Changes

- The fluid mesher defers flat, still fluid top faces and merges identical neighbours into rectangles of at most 3x3 blocks, with the sprite repeating once per block.
- Merging requires equal Y, corner height, sprite, light, back-face flag, translucency and resolved biome tint colour of the quad's tint column.
- Flowing (rotated UV) and sloped top faces, side faces and bottom faces are unchanged.

## Out of scope

- Merging side faces, and merging beyond 3x3 (needs a wider model quad position range).

## Impact

- Affected specs: `chunk-meshing` (Fluids). No format or shader change.
