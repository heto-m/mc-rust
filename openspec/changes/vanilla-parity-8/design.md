## Context

Vanilla references are `Class#method` in the decompiled 26.3 client under `.cache/`. Run the font
harness with `cargo xtask parity font`; its full findings are in `differential-parity/design.md`.
Severity: **G** gameplay-breaking, **V** visible, **M** minor.

## Gap catalogue

### Text (font harness)

- **T1 (V) Chat continuation indent.** `ComponentRenderUtils.wrapComponents` starts every wrapped
  continuation line with a space. `ChatHud` wraps without it. About 1550 cases.
- **T2 (V) `§r` after a split.** `StringSplitter.splitLines` resets `§r` to `Style.EMPTY`. Each wrapped
  line carries the style active at its break. Ours resets to the component's own style. About 255 cases.
- **T3 (M) Unknown font ids.** Vanilla draws missing glyphs (advance 6, bold 7). `FontId::from_name`
  falls back to the default font. 89 cases.
- **T4 (M) Trailing invisible codes.** Vanilla emits a final empty line. 10 cases.
- **T5 (M) Empty chat message.** Vanilla adds one empty line. 2 cases.
- **T6 (M) `§` before a supplementary character.** Vanilla uses the high surrogate as the code and draws
  U+FFFD. 2 cases.

### Screens and window (benchmark)

- **S1 (V) No title panorama.** Vanilla's title screen draws the rotating cubemap panorama from
  `textures/gui/title/background/panorama_*.png`, blurred, scrolling at `panoramaScrollSpeed`. Ours
  draws a flat gradient.
  Findings: the client jar carries 1x1 placeholder faces; the real ones come from the launcher asset
  index, so `AssetIndexSource` now serves `textures/gui/title/background/`. `CubeMapTexture` stacks the
  faces with `mirrorY`. The title screen has no blur (`TitleScreen#extractBackground` is empty); only
  screens opened over the panorama blur it, and those still draw the flat menu background.
- **S2 (M) Windowed `--size` ignored.** The window opens at 1280x720 whatever the flag says.
  Vanilla's `--width`/`--height` set the initial window size.

### Text components (text harness, 58.5% of 4000)

- **X1 (V)** `to_plain_string` ignores the language: it prints the fallback or the key where vanilla's `getString` resolves through `Language`. 1204 cases.
- **X2 (V)** Missing or out-of-range arguments: vanilla's `TranslatableContents.decompose` falls back to the raw template; ours drops the placeholders. 609 cases.
- **X3 (M)** Score contents: vanilla draws nothing on the client; ours draws the holder name. 355 cases.
- **X4 (M)** NBT contents: vanilla draws nothing on the client; ours draws the path. 291 cases.
- **X5 (M)** Unsupported format specifiers (`%d`, others, a stray `%`): vanilla renders the raw template. 294 cases.
- **X6 (M)** `%0$s`: vanilla treats it as invalid; `flatten_chat` uses argument 0. The protocol-side panic is fixed in bc0fde3. 160 cases.
- **X7 (M)** Double arguments use Java's `Double.toString` (`2.63E9`). 26 cases.
- **X8 (M)** `%%` is a separate piece in vanilla, so legacy codes do not carry into it. 5 cases.

### Section meshing (section_mesh harness, 2.75% of 400)

- **M1 (V)** Fluid quads: positions are quantised to 1/128 and lose `FluidRenderer`'s 0.001 insets; UVs are on 1/16 of a tile. 124k quads.
- **M2 (V)** Per-face render layer: vanilla's `FaceBakery.computeMaterialTransparency` judges only the sprite region a face covers, so cauldron, hopper, iron bars, lantern, torch, scaffolding and door faces are SOLID. Ours puts them in cutout. 48.9k quads.
- **M3 (M)** Model quad positions are quantised to 1/128. 23.6k quads.
- **M4 (V)** Lava layer: vanilla derives it from sprite transparency, so lava is SOLID; ours is cutout. 7.6k quads.
- **M5 (M)** Flat light: `Mesher::push_model_quad` blends corner light in f32, so off-grid vertices land one unit low. Vanilla's `prepareQuadFlat` uses a single value per quad. 2.5k quads.
- **M6 (V)** Faces against stairs never cull. Vanilla's `Block.shouldRenderFace` culls them against a stair's full faces. 335 quads.
- **M7 (V)** Legacy solidity ignores `forceSolidOn`/`forceSolidOff` and the rule that `dynamicShape` blocks are not solid. This changes fluid corner heights next to lanterns, ladders, snow and scaffolding. 322 quads.
- **M8 (M)** Snow smooth light: one vertex differs by one unit. Not traced.
- **M9 (V, open)** `FlowingFluid.getFlow` tests the server tag `#blocks_fluid_flow` (`blocks_motion` plus
  all signs); ours approximates it with legacy solidity, so flow direction can differ next to signs. No
  harness case covers it.
- **M10 (V, open)** Iron bars and door windows show dark texels along cutout edges that vanilla does not
  (live screenshot, same before and after this batch). Texture sampling, not meshing.

### Carried from vanilla-parity-7

- **C1 (M)** stop_sound in mod mode, once mods resume.
- **C2 (M)** Cape on elytra, which needs the owner's online-mode account.
- **C3 (M)** The `local_difficulty`, `entity_spawn_counts` and `chunk_generation_stats` F3 entries, which
  need a server level.
- **A1 (V)** A full model arena drops the section (`RenderError::ArenaFull`, logged once), which leaves a hole in the world. Batch 5 cut the arena to 4M quads, about 1.4x the bench forest at render distance 32. The arena must grow, or be sized from the render distance, instead of dropping sections.
- **C4 (M)** The language-aware plain string (`to_plain_string_with`) is not yet threaded through the `mcr-app` and `mcr-client` callers: shell, hud, nametags, entities, map, editors, the sign character limit in `mcr-block-entity`, `dialog_screen` option and slider labels, and `tooltip::first_plain`. This affects emptiness checks, the sign limit and logs.

## Decisions

### Section meshing

- **Model quads carry `f32` positions and UVs (96 B, was 40 B).** Vanilla's positions are `f32` with bits
  below 2^-20 (rotated elements, the fluid renderer's 0.001 insets), so no fixed grid is bit-exact.
  Measured on the harness with section-local fixed grids: positions at 2^-12, 2^-14 and 2^-16 pass 6%,
  13.5% and 14.25% of cases, 2^-18 and finer pass all; UVs at 2^-12 pass 85%, 2^-14 99.5%, 2^-16 all.
  The cheapest passing grid would pack into 64 B, but it passes only because the harness rounds to
  1/1024 (a 1/1024 grid also passes, trivially), so it is not exact. `f32` is exact and costs no GPU
  time, so the format follows vanilla. Cube quads stay 16 B. Harness tolerances are unchanged.
- **Cost**, bench world at 1920x1080, render distance 16, 600 frames, median of 3 interleaved idle runs
  (main vs this batch): GPU frame 0.203 / 0.169 ms vs 0.204 / 0.169 ms on the two views; model meshes
  28.8 MB vs 69.2 MB (about 2.2 KB vs 5.4 KB per meshed section), cube meshes 41.0 MB both; upload
  21.5 ms vs 25.6 ms for a full load. Single-thread meshing (criterion `mesh_surface_sections`)
  7.9 ms vs 7.5 ms. The world model arena drops from 8M to 4M quads so its preallocation stays near the
  old size (384 MB vs 320 MB); render distance 32 over the bench forest needs about 2.9M.
- **Per-face render layer** (M2) judges the face's UV rectangle on the sprite's alpha across all frames,
  like `FaceBakery.computeMaterialTransparency`. Fluids (M4) take their layer from their sprites.
- **Legacy solidity** (M7) comes from vanilla: the extractor and the bridge registry export
  `BlockState.isSolid()` as `LEGACY_SOLID` (bit 10, no ABI bump). The shared
  `.cache/versions/26.3/extracted/blocks.json` was regenerated with the new field.
- **Smooth light and fluid flow** follow vanilla's arithmetic: `BlockModelLighter`'s weighted blend in
  its summation order with integer truncation (M8), flat light from the raw neighbour value plus own
  emission (M5), and `getFlow` in `f64` with `Mth.atan2` and the `Mth.sin` table for flowing UVs.
- Live check against the vanilla client (lanterns, doors, iron bars, lava, water; day and night): mean
  pixel difference outside the animated fluids 0.45 (main: 1.23) by day and 0.51 at night; what remains
  is M10.

