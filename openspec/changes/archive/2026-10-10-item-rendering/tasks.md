# Tasks

## 1. Assets: definitions and evaluation

- [x] 1.1 Parse `items/*.json` into a typed tree; test: every jar definition parses
- [x] 1.2 Evaluator with `ItemProps`/`ItemContext`; tests: bow, compass, clock, potion, leather dye, trim select, chest special
- [x] 1.3 Parse `display`, `gui_light` and builtin parent kind in the model loader; test: `block/cube_all` gui transform

## 2. Assets: baking

- [x] 2.1 Bake `builtin/generated` into front/back/edge quads; test: single-pixel sprite
- [x] 2.2 `ItemModelBaker` baking item models with display transforms; test: all jar items evaluate and bake with default context

## 3. UI: icons and decorations

- [x] 3.1 CPU rasteriser and icon cache in the sprite atlas; test: cube silhouette and cache reuse
- [x] 3.2 Count, durability bar, cooldown overlay; unit tests on bar width and colour
- [x] 3.3 Glint pipeline and time uniform; offscreen render shows glint

## 4. HUD

- [x] 4.1 Hotbar and offhand items; offscreen PNG with the requested items checked visually

## 5. Verification

- [x] 5.1 `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
