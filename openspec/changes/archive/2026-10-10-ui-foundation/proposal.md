## Why

The client has a 3D renderer but no 2D layer. HUD, chat, menus and the F3 overlay all need vanilla-faithful text and sprite drawing, so the foundation has to exist before any screen is built.

## What Changes

- New crate `mcr-ui`: GPU 2D layer on wgpu, fed by the user's own client jar through `mcr-assets`.
- Fonts: vanilla font definitions (`default`, `uniform`, `alt`, `illageralt`) with bitmap, space, unihex and reference providers, lazily built glyph atlas pages, vanilla glyph metrics and effects (shadow, bold, italic, underline, strikethrough, obfuscated).
- Text layout: `Component` flattening with legacy `§` codes, hex colours, vanilla shadow colours, `en_us.json` translation, width measurement and `StringSplitter`-style word wrap.
- Sprites: GUI sprite atlas with stretch, tile and nine-slice scaling from `.mcmeta`.
- Renderer: immediate-mode batched `UiFrame` (fill, sprite, text, component, scissor, auto GUI scale), pipeline built from a `wgpu::Device` and target format so the app can composite it after the 3D pass.
- Proof screens: F3 debug overlay, chat HUD with fade, hotbar/health/hunger/xp HUD.

Out of scope: widgets, input handling, screens with state, item/entity rendering in the GUI, TrueType and player/atlas glyph providers, bidi/Arabic shaping, game state plumbing, wiring into `mcr-app`.

## Capabilities

### New Capabilities
- `ui-text`: font loading, glyph metrics, text layout and wrapping.
- `ui-render`: sprite atlas, batched 2D renderer and the proof HUD screens.

### Modified Capabilities

## Impact

New crate `crates/mcr-ui` depending on `mcr-assets`, `mcr-protocol` (read-only) and `wgpu` 30. No changes to existing crates.
