# Tasks

## 1. Render formats and shaders
- [x] 1.1 12-byte cube and 40-byte model quad formats, encoders, WGSL decoders, GPU decode test
- [x] 1.2 Environment, lightmap uniform, per-vertex lighting in the shader, fog, clear colour
- [x] 1.3 Animation table, `set_animations` / `set_animation_time`, shader interpolation
- [x] 1.4 UV rotation for cube quads

## 2. Mesher
- [x] 2.1 Per-corner light and AO (cube and model), vanilla diagonals, merge keys
- [x] 2.2 Variants (Java random, seeds), offsets, rotation, cache changes
- [x] 2.3 Occlusion masks and `shouldRenderFace`
- [x] 2.4 Fluid rewrite
- [x] 2.5 Biome zoom, per-column tints, climate parsing

## 3. Verification
- [x] 3.1 Unit tests (light/AO fixtures, seeds, fluid heights, occlusion), real-jar all-states test
- [x] 3.2 Screenshots before/after, benchmark (single-thread section < 120 us)
- [x] 3.3 fmt, clippy, tests, archive
