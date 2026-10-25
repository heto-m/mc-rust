## 0. Change

- [x] 0.1 Proposal, design, specs and tasks; `openspec validate differential-parity --strict`

## 1. Framework

Scope: new `crates/mcr-parity`, new `java/mcr-oracle`, `xtask`.

- [x] 1.1 Case and result formats, seeded generators, the diff with the first-mismatch report (D1)
- [x] 1.2 `java/mcr-oracle` built by `cargo xtask java-build` against the client jar; bootstrap helper (D2)
- [x] 1.3 `cargo xtask parity` with selection, oracle caching, `status.json`, `unshare -rn` and the memory scope (D3)
- [x] 1.4 First harness end to end: font width and text wrapping over generated strings and styles
- [x] 1.5 fmt, clippy and workspace tests

## 2. Pure client logic

One harness each; record case count and pass rate. Failures are filed, not fixed here.

- [ ] 2.1 Text components: style merge, legacy codes, translation with arguments, component to plain and to draw runs
- [ ] 2.2 Block model baking: quads per block state (faces, UV, rotation, tint index, shade, cullface)
- [ ] 2.3 Section meshing: generated sections incl. fluids, AO, smooth lighting, culling, tint
- [ ] 2.4 Item models: model selection and layers over generated stacks with components
- [ ] 2.5 Tooltips: lines and styles for generated stacks with components, advanced tooltips on and off
- [ ] 2.6 options.txt and key mappings: parse, defaults, round-trip
- [ ] 2.7 Entity model poses: `setupAnim` part transforms over generated render states
- [ ] 2.8 Local player physics: move and collide over block scenes and input sequences (stairs, slabs, ladders, fluids, sneaking edges, elytra)

## 3. Draw-stream harness

- [ ] 3.1 Headless vanilla boot without a loader on the recording GPU backend (D4)
- [ ] 3.2 Normalized stream recorder on the Java backend and in `mcr-ui` / `mcr-render` behind a feature
- [ ] 3.3 GUI scenes: title, options pages, pause, inventory, creative tabs, recipe book, chat, F3, HUD states, toasts
- [ ] 3.4 World scenes on a local server at fixed cameras: terrain, block entities, entities

## 4. Frame harness

- [ ] 4.1 Vanilla frame replayed through our wgpu backend and our native frame at the same camera (D5)
- [ ] 4.2 Pixel diff, diff images, per-frame score; sky, fog, weather, particles, post effects

## 5. Gate and score

- [ ] 5.1 `merge.sh` runs changed harnesses with known failures (D6)
- [ ] 5.2 Parity table in the `differential-parity` spec; file mismatches into `vanilla-parity-8`, with the vanilla-parity-7 close-out leftovers

## 6. Close out

- [ ] 6.1 Validate and archive the change
