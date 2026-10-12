## Why

The post-effects and parity-followups-1 changes left four known gaps against vanilla 26.3: text display entities were not drawn, the code-of-conduct "do not notify again" option was a bracket button, resource packs could not retune the vanilla post chains outside mod mode, and the window present mode's post path had never run live.

## What Changes

- Text display entities draw like `TextDisplayRenderer`: wrapped lines, alignment, shadow, see-through, background (default or custom), text opacity and the billboard modes. Their layout is cached per text change and drawn as glyph geometry.
- All display entities (block, item, text) interpolate their transformation, and text displays interpolate opacity and background, with vanilla's start delay and duration. They also use the brightness override and the view range and culling box of the entity data.
- `mcr-ui` gains a `Checkbox` widget with vanilla's four sprites, toggled by click, Enter and Space. The code-of-conduct screen uses it.
- A resource pack's `post_effect/<chain>.json` for `blur`, `creeper`, `invert` or `spider` retunes our WGSL passes when it has the built-in chain's shape. Other shapes keep the built-in chain, with a logged warning.
- The window present mode with a post chain was run live under Xvfb; the result is recorded in the design.

## Impact

- Affected specs: `entity-rendering`, `ui-widgets`, `post-effects`.
- Code: `mcr-entity-model` (display interpolation), `mcr-block-entity` (text layout, display text pipelines), `mcr-ui` (checkbox, conduct screen), `mcr-render` (pack-tuned chains), `mcr-app` (entity layer, text displays, post effects wiring).
