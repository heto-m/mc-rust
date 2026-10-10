# Proposal

## Why

The HUD has no items. Vanilla describes how every item looks in `items/*.json` (a tree of conditions, selects, range dispatches and tints over `item/*` and `block/*` models); without evaluating it we cannot draw hotbar, inventory or tooltip icons.

## What Changes

- `mcr-assets` parses `assets/*/items/*.json` into an evaluable tree and evaluates it against a small `ItemProps` trait plus an `ItemContext`, yielding model layers with tints or a typed special-model marker.
- `mcr-assets` resolves `display` transforms and `gui_light` on models, and bakes `builtin/generated` layer models into extruded quads (front, back, alpha-derived edge faces) and plain element models via the existing baker.
- `mcr-ui` renders items into 16x16 GUI slots: each distinct variant is rasterised once into the sprite atlas (flat sprites and `gui`-transformed 3D blocks with the two GUI light vectors), invalidated by rebuilding on resource reload.
- `mcr-ui` draws item decorations: count text, durability bar, cooldown overlay and an animated enchantment glint pass.
- The hotbar and offhand draw items from a slot list passed beside `HudState`.

Out of scope: inventory screens, tooltips, in-world item entities and first/third-person item rendering, entity-model special renderers (chests, shulker boxes, heads and banners are approximated from entity textures; shields, pots, conduits, statues show the particle texture), potion colours derived from the effect registry, and map/player-head textures.

## Capabilities

### New Capabilities
- `item-models`: item model definition parsing and evaluation, model display transforms, generated-model baking.
- `item-gui`: GUI item icons, decorations, glint and hotbar items.

### Modified Capabilities

## Impact

`crates/mcr-assets` (new `item` modules, `model.rs` display/builtin parsing), `crates/mcr-ui` (new `items` module, glint pipeline in `renderer.rs`, hotbar drawing, `mcr-data` dependency). No changes to other crates.
