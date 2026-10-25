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
- **S2 (M) Windowed `--size` ignored.** The window opens at 1280x720 whatever the flag says.
  Vanilla's `--width`/`--height` set the initial window size.

### Carried from vanilla-parity-7

- **C1 (M)** stop_sound in mod mode, once mods resume.
- **C2 (M)** Cape on elytra, which needs the owner's online-mode account.
- **C3 (M)** The `local_difficulty`, `entity_spawn_counts` and `chunk_generation_stats` F3 entries, which
  need a server level.
