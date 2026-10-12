## 1. Texture-layer leak

- [x] 1.1 Free list per size class with frames-in-flight deferral in `EntityRenderer`
- [x] 1.2 `TextureCache`: free on replace, idle and LRU eviction of `dyn:` skins and trims, map removal
- [x] 1.3 Tests: churn 10k dyn keys, replace and reuse deferral, double free
- [x] 1.4 Block-entity skull skin cache
- [x] 1.5 UI atlas: free and reuse dynamic sprite slots; bound icon, favicon and head caches
- [x] 1.6 Audit the remaining texture allocators (render, particles, modbridge, flat)

## 2. Display teleport duration

- [x] 2.1 Position and rotation interpolation

## 3. Text display draw order

- [x] 3.1 Translucent pass order or documented difference

## 4. Dialog checkbox

- [x] 4.1 Dialog screens use `Checkbox`

## 5. Block markers

- [x] 5.1 `block_marker` particles
- [x] 5.2 Creative barrier and light markers

## 6. Campfire smoke

- [x] 6.1 Food smoke from the item slots

## 7. Client ticks

- [x] 7.1 Conduit, spawner, vault and trial spawner (check first)

## 8. Close out

- [x] 8.1 Merge main, fmt, clippy, tests
- [x] 8.2 Validate and archive the change
