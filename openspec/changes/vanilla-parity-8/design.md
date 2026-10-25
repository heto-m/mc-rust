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

### Carried from vanilla-parity-7

- **C1 (M)** stop_sound in mod mode, once mods resume.
- **C2 (M)** Cape on elytra, which needs the owner's online-mode account.
- **C3 (M)** The `local_difficulty`, `entity_spawn_counts` and `chunk_generation_stats` F3 entries, which
  need a server level.
