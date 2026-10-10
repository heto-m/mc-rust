# Design

## Context

`mcr-assets` already resolves block/item JSON models (`ModelLoader`) and bakes block states; `mcr-ui` has a sprite atlas and a batched premultiplied renderer. See proposal.md.

## Goals / Non-Goals

**Goals:**
- Evaluate all 26.3 item definitions without protocol dependencies in `mcr-assets`.
- Draw icons through the existing batched pipeline with at most one extra pipeline (glint).

**Non-Goals:**
- Entity-model special renderers, in-world item rendering.

## Decisions

- `ItemProps` trait with defaulted methods (damage, dyed colour, potion colour, custom model data, trim material, component presence/strings). `mcr-ui` adapts `ItemStack`. This keeps `mcr-assets` free of `mcr-protocol`.
- Node properties are parsed once into typed variants; unknown properties are parse errors so jar coverage is checked by test.
- Icons are rasterised on the CPU into the UI sprite atlas (software z-buffer, nearest sampling, 16*scale pixels square) rather than an offscreen GPU pass: no extra render targets, deterministic and testable without a GPU, and slots batch with the rest of the UI. Alternative: GPU offscreen icon atlas; rejected for the added pass/sync cost on tiny icons.
- Lighting follows the vanilla item shader with the two fixed GUI light vectors; `gui_light: front` uses flat lighting.
- Glint: a second pipeline draws the icon quad again, using the icon alpha as silhouette mask and a repeating glint texture scrolled by time, added over the framebuffer.
- Texture sources are read directly from the `ResourceManager`; the first animation frame is used.

## Risks / Trade-offs

- [Potion colours need the effect registry] → use `custom_color` or the definition default.
- [Per-scale icon cache grows with GUI scale changes] → cache keyed by scale, cleared on scale change.
- [Special models show the base icon] → documented gap.
