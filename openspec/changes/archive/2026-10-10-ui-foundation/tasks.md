## 1. Scaffold
- [x] 1.1 Create `crates/mcr-ui` with dependencies and lib layout

## 2. Fonts
- [x] 2.1 Font definition parsing, providers (bitmap, space, unihex, reference), filters
- [x] 2.2 Glyph metrics and missing glyph; lazy shelf-packed atlas pages
- [x] 2.3 Unit tests for metrics ('i', 'W', space, bold)

## 3. Text
- [x] 3.1 Component flattening, `§` codes, colours, shadow colour, lang resolution
- [x] 3.2 Width measurement and `StringSplitter` wrapping, with tests

## 4. Sprites
- [x] 4.1 Sprite atlas and mcmeta scaling; geometry tests

## 5. Renderer
- [x] 5.1 `UiFrame` batching, scissor, GUI scale, shader and pipeline
- [x] 5.2 Text drawing with effects

## 6. Screens
- [x] 6.1 Debug overlay, chat HUD, hotbar HUD
- [x] 6.2 Offscreen PNG tests, inspected visually
- [x] 6.3 Criterion bench: layout and batch 200 chat lines

## 7. Wrap-up
- [x] 7.1 fmt, clippy, tests; archive change
