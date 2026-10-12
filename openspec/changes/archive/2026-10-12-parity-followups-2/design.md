## Context

Follow-ups left by `post-effects` and `parity-followups-1`:
- text displays were not drawn;
- the code-of-conduct "do not notify again" option was a button labelled `[ ]`;
- resource packs that override vanilla's post chains were followed only in mod mode;
- the window present mode's post path was never run live.

## Text displays

Vanilla's `TextDisplayRenderer` splits the component into lines, then submits one background quad and one text run per line. mcr does the same with its own pieces.

**Layout.** `mcr_block_entity::layout_display` flattens the component into cells, wraps with the UI's `wrap` (a port of `StringSplitter.splitLines`) and lays each line out through the UI's glyph batch:
- line `i` sits at y = `i * 10`; the box height is `lines * 10 - 1`;
- the box width is the widest line, rounded up;
- the line offset is 0 (left), `width - line` (right) or `width / 2 - line / 2` (center);
- the shadow flag is the font's normal shadow.

The glyph quads are collected from the batch (positions, atlas uvs, colour), so the text is real geometry on the font atlas and stays crisp at any distance.

**Bounds.** Everything is server-driven, so it is bounded: 4096 cells (and the existing component expansion budget), a line width clamped to 1..1024 and at most 64 lines. A layout is cached per entity and rebuilt only when the text, the line width, the alignment or the shadow flag changes. At most 256 layouts and 128 draws exist per frame.

**Placement.** The display-local matrix is the billboard orientation times the interpolated transformation (the same `display_local` block and item displays use). The text pose is vanilla's: rotate pi about Y, scale -0.025, translate `(1 - width / 2, -height)`. The text reads from the display's +Z side and shows mirrored from behind, as in vanilla. The background spans `[-1, width] x [-1, height]` at z -0.01, the text at z 0.

**Pipelines.** Two new flat pipelines in `mcr-block-entity`, drawn with the blended terrain-translucent pass:
- `DisplayText`: depth test without writes, with a depth bias toward the camera (vanilla's polygon offset);
- `DisplayTextSeeThrough`: no depth test.

A new `Background` bind selects the white pixel of glyph page 0 and sorts before the glyph pages, so the background is always under the glyphs.

**Colours.**
- Text alpha is the opacity byte; below 4 it draws opaque, as `Font.adjustColor` does.
- The background is the entity colour, or black at `getBackgroundOpacity(0.25)` when the default-background flag is set. That is 0.25 while "background for chat only" is on, otherwise the text background option.
- Both are shaded by the entity's light.

**Why the block-entity crate.** Its flat pipeline already draws sign text from the font atlas pages. The entity renderer's size-class texture arrays cannot hold the font atlas, and its texture layers are never freed, so per-text textures would leak. `EntityLayer` computes the layouts and hands the draws to the block-entity scene each frame.

## Display interpolation

`mcr_entity_model::DisplayTrack` ports the client half of `Display.tick`:
- It compares the synced values each tick, standing in for `onSyncedDataUpdated`, and rebuilds its interpolation pair when a render-state field changed.
- A changed start delta sets `start = tick + delta`; a changed duration is kept.
- When a duration is set and a state exists, the new pair starts from the state sampled at the last rendered progress.
- Progress is `(tick - start + partial) / duration`, clamped, and 1 without a duration.
- Transformations interpolate by lerp (translation, scale) and slerp (the two rotations).
- Opacity is a signed byte lerped as an int. The background is lerped per ARGB channel with `Mth.lerpInt`.

Block and item displays take the sampled matrix too (before, they read the raw value). Displays also use the brightness override and apply vanilla's range rule (`viewRange * 64 * scaling`, frustum box only when width and height are non-zero). Before, they used the 0x0 type box and were culled beyond 16 blocks.

Hostile input: transformation components clamp to 64 and non-finite values become 0; rotations that are non-finite or near zero length become the identity; the interpolation duration clamps to 2^20 ticks.

## Checkbox

`Checkbox` in `mcr-ui` is a widget in the `WidgetSet`: 17x17 box from the four `widget/checkbox*` sprites, the label four pixels to the right, `Toggled(bool)` on click, Enter or Space. The conduct screen sizes it to the label and centers it. The dialog screens keep their own checkbox drawing for now; they share the sprites and the metrics.

## Resource-pack post chains outside mod mode

Vanilla loads `post_effect/<id>.json` from the resource stack, so a pack can re-tune a chain or replace it with its own GLSL. We cannot run pack GLSL. `PostOverrides` follows a pack file only where it maps onto our WGSL passes' parameters:
- the pass count, the vanilla fragment and vertex shader ids, the input and output targets and the filters equal the built-in chain's;
- targets are empty definitions (so no custom sizes);
- every uniform the pass reads is present with the right arity; values are clamped (blur radius 0..32, invert -2..2, tint 0..8, bits 1..256, mosaic 1..64 and so on).

Then the values replace the built-in constants. Any other file is ignored with a warning: `entity_outline`, chains with pack shaders, restructured graphs, extra passes, sized targets. The built-in chain then runs.

The resource stack is read at session start and on reload. Because mcr owns these four chain ids in mod mode too (Java skips them), the same overrides apply there.

A test reads the real vanilla `post_effect` files and checks they reproduce the built-in chains exactly, which is also what keeps the WGSL constants honest.

## Window present mode live check

Result: not run.
- Xvfb is installed. `Xvfb :99` was started and mcr was run with `DISPLAY=:99`, `WAYLAND_DISPLAY` unset and `WINIT_UNIX_BACKEND=x11`.
- Renderer creation failed with `MESA: info: vulkan: No DRI3 support detected - required for presentation`, then `no suitable GPU adapter: vulkan not compatible with provided surface ... gl not compatible with provided surface`. The same happened with `WGPU_BACKEND=gl`, `LIBGL_ALWAYS_SOFTWARE=1` and EGL x11.
- Xvfb cannot offer DRI3, so no Vulkan surface can be created on it. There is also no in-app screenshot key for a surface target (`read_offscreen` needs the offscreen image).
- The window post path is therefore still covered by the renderer wiring test only. A virtual Wayland compositor (a headless kwin or weston) would give it a surface, but those can claim the session bus or the real display, so it is left for a manual run.

## Verification

Live, against a vanilla 26.3 server with offscreen screenshots:
- Text displays with wrapping, left and right alignment, shadow, gold bold text, a custom green background, opacity 128 with the default background, and see-through text behind a stone wall all draw as expected.
- A text display with `interpolation_duration` grew and moved smoothly after a `data merge` of its transformation.
- `/posteffect add` of spider and invert still processes the world, text displays included, after the post stack refactor.

## Risks

- Text draws in the translucent pass after terrain, so it does not interleave with water or glass the way vanilla's sorted submits do.
- Position and rotation interpolation of displays (`teleport_duration`) is not implemented: displays still move over one tick.
