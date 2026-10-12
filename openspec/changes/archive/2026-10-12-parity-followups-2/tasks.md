## 1. Text displays

- [x] 1.1 Display interpolation state (`DisplayTrack`): change detection, start delay, duration, signed-byte opacity, ARGB background, hostile transform clamps
- [x] 1.2 Block and item displays use the interpolated transformation, the brightness override, the view range and the culling box
- [x] 1.3 Text layout: wrapping, alignment, shadow, line, cell and width bounds (`layout_display`)
- [x] 1.4 Text display draws: background, opacity, see-through and offset pipelines, per-entity layout cache
- [x] 1.5 Tests: layout bounds, hostile text, offscreen pixels, see-through behind a wall
- [x] 1.6 Live check against a vanilla server: wrap, alignment, shadow, colours, opacity, default background, see-through, interpolation

## 2. Checkbox

- [x] 2.1 `Checkbox` widget with the four sprites, click, Enter and Space
- [x] 2.2 Code-of-conduct screen uses it

## 3. Resource-pack post chains

- [x] 3.1 `PostOverrides`: follow packs of the built-in shape, clamp values, ignore the rest
- [x] 3.2 Wire the resource stack into the renderer on start and on reload
- [x] 3.3 Tests: retuned chain, other shapes, hostile values, the vanilla files reproduce the built-in chains

## 4. Window present mode

- [x] 4.1 Run the window post path under Xvfb (or record why it could not run)

## 5. Close out

- [x] 5.1 Merge main, fmt, clippy, tests
- [x] 5.2 Validate and archive the change
