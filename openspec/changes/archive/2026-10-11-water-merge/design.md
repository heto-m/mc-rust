## Decisions

- **Deferred emission.** Water tint is a per-column colour table filled after the section is meshed, so equality of tint cannot be decided while meshing. Eligible tops go to a per-mesher buffer and are merged and emitted after the table is built, comparing the table value of each cell's column.
- **Eligibility.** All four corner heights equal (flat) and the unrotated still UVs. Sloped or flowing tops keep one quad per block.
- **3x3 cap.** A model quad stores corners as 9-bit offsets (1/128 block, biased by one block) from its anchor cell, so a quad reaches at most -1..2.99 blocks around it. Anchoring on the second cell of a run allows a span of 3. Widening it would change the 40-byte format and shader; not worth it for a further 2-3x on an already ~7x reduction. The cap also keeps UVs far below the 8-bit limit (15.9).
- **Texture tiling.** UVs run 0..W by 0..D instead of 0..1 per block. The block sampler uses `Repeat` addressing and every sprite is its own array layer, so the repeat reproduces N separate quads exactly; animation interpolates per layer in the fragment shader and is unaffected.
- **Tint.** The shader samples the tint table once per quad from the anchor cell. The anchor lies inside the run and all columns of a merged rectangle have the same resolved colour, so the result equals the unmerged one. Smooth biome blending that varies per column simply splits the merge.
- **Translucent sorting.** mcr-render sorts translucent draws per section (back to front by section distance); quads inside a section are not sorted. A merged quad therefore cannot change ordering relative to other geometry, and the 3x3 cap bounds how much of a section one quad covers anyway.
- **Lava** goes through the same path (opaque model layer, no tint); merging is identical.

## Risks

- Output quad order within a section changes (tops are emitted last). Nothing depends on it.
