## Why

Texture-array layers were allocated and never freed: a replaced skin or map, an idle player skin or armor trim, and every UI atlas slot of a dropped icon stayed allocated for the whole session. On a long session on a busy server the layers run out and new textures fall back to the missing texture. Several smaller vanilla-parity gaps were also left by `parity-followups-2` and `block-animate-ticks`.

## What Changes

- The entity renderer's texture arrays free layers through a per-size-class free list. A freed layer is reused only after the frames in flight have passed.
- `TextureCache::replace` frees the old layer. `dyn:` skin and trim textures are evicted when idle or past a bounded count (LRU). Map textures are freed when their map is evicted.
- The block-entity skull skin cache and the UI sprite atlas (favicons, head sprites, item icons) free what they drop and are bounded.
- Display entities interpolate position and rotation over `teleport_duration`.
- Text displays draw in the translucent pass order, or the remaining difference is documented.
- Dialog screens use the `Checkbox` widget.
- `block_marker` particles render, and creative mode shows vanilla's barrier and light markers.
- Campfire food smoke uses the block entity's item slots.
- Conduit, spawner, vault and trial spawner client ticks are added where missing.

## Impact

- Affected specs: `entity-rendering`, `ui-render`, plus those that the later items touch.
- Code: `mcr-entity-model` (gpu, texcache, scene, maps), `mcr-block-entity` (skin cache), `mcr-ui` (atlas, sprites, icons, dialogs), `mcr-animate`, `mcr-particles` and `mcr-app` for the parity items.
