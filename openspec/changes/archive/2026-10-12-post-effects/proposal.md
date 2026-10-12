## Why

Vanilla post-processes the world image before the GUI. Two things select the post chains: the server's `post_effects` packet (`/posteffect`), and the camera entity (spectating a creeper, spider or enderman). mcr ignored the packet and had no intermediate image to process, so spectator views and `/posteffect` looked plain.

## What Changes

- `mcr-client` keeps the server's post effect list. The list is bounded, and it is cleared on respawn, as vanilla's new `LocalPlayer` clears it.
- `mcr-render` gains a post stack:
  - our own WGSL passes for vanilla's `blur`, `creeper`, `invert` and `spider` chains;
  - each chain is a small pass graph over pooled screen-sized targets;
  - ids outside that set are ignored, as vanilla ignores chains it cannot load.
- The renderer runs the chains after the world, the world-space overlays and the hand, and before the UI. This works in all three present modes:
  - offscreen and scene-texture targets are processed in place;
  - a window without a scene texture renders into an intermediate image only while a chain is active, then blits it.
- `ExternalSubmitter` gains `before_ui`. In mod mode the chain runs after Java's world slots and before Java's GUI slot. Java skips the vanilla chains that mcr now runs, and keeps the chains only Java knows (mod or resource-pack chains).
- `mcr-app` derives the spectated-entity chain like vanilla:
  - it is set when the camera changes and cleared in third person;
  - `key.toggleSpectatorShaderEffects` (F4) toggles it;
  - `blur` uses the menu background blurriness option.
- `/posteffect` suggestions list the vanilla chain ids.
- Glowing entity outlines: mcr's native renderer had none, so vanilla's `entity_outline` chain is ported over silhouettes drawn by the entity renderer in team colours.

## Out of scope

- Loading post chain JSON or GLSL from the client jar or resource packs (custom chains outside mod mode).
- `end_of_frame`: vanilla ships no such chain, and only resource packs define it.
- Menu background blur for mcr's native screens.

## Impact

- Affected specs: new `post-effects`.
- Code:
  - `mcr-render`: post stack, renderer present path, `ExternalSubmitter`;
  - `mcr-client`: state, handler, suggestions;
  - `mcr-app`: play;
  - `mcr-modbridge`: `before_ui`;
  - `java/mcr-bridge`: skips the chains mcr owns.
