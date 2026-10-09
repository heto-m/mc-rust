## Context

Vanilla draws text through per-font atlas pages with 1px spacing and nudged UVs. We reimplement behaviour, not code, and read all assets from the user's jar.

## Decisions

- Text is flattened to `Vec<Cell { ch, Style }>` with a compact `Copy` style; wrapping and drawing operate on slices. This matches `StringSplitter` semantics without allocating per line.
- Glyph pages are 256x256 RGBA8 shelf-packed with 1px spacing, filled lazily; dirty pages upload once per frame. Grayscale bitmap glyphs are stored as white with alpha and tinted by vertex colour; coloured glyphs (unihex, special) are likewise white-on-alpha, which is what vanilla renders.
- One shader and pipeline for everything: vertex = position (GUI px), uv, colour. Fills sample a white pixel. A batch breaks on page or scissor change.
- Premultiplied alpha: shader outputs `rgb*a, a`; blend `One, OneMinusSrcAlpha`.
- Unihex needs `unifont.zip`, which ships in the asset index and not the jar. The provider is implemented and tested on synthetic data; with the jar alone code points beyond the bitmap providers render as the missing glyph.
- TrueType, `player` and `atlas` providers are skipped with a warning.
- Obfuscated text picks a pseudo-random glyph of equal advance per draw.
- `mcr-ui` is renderer-agnostic: callers pass `&wgpu::Device`, `&wgpu::Queue` and a `TextureFormat`, and encode into their own render pass.

## Risks

- No depth buffer, so vanilla's z-ordered shadow, glyph and effect layers are replicated by emit order.
