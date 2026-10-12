## Context

`EntityRenderer` keeps one texture array per size class (64 to 1024). `add_texture` bumped a counter and nothing ever gave a layer back, so every replaced map, changed skin or trim that was composed once stayed for the session. The array has a hard layer cap per class (and the device limit), after which `add_texture` returns `None` and callers fall back to the missing texture.

## Layer freeing

Each class array keeps a live bitmap, a free list and a queue of retired layers stamped with the frame they were freed on. `free_texture` ignores layers that are not live (double frees cannot hand one layer to two textures). `add_texture` first moves retired layers older than `FRAMES_IN_FLIGHT` (4) frames to the free list, then reuses a free layer before growing. The class cap is also clamped to the device's `max_texture_array_layers`; before, growing past a low device limit was a validation panic.

## Texture cache

- `replace` drops its still-pending upload of the same key and the flush releases the old layer after the new upload succeeds.
- Skins (`dyn:skin:`) and trims (`dyn:trim:`) are re-registered by their planners whenever they are missing, so they may be evicted. A sweep every 64 frames frees those unused for 1800 frames. When more than 384 exist the least recently used ones not used this frame are evicted down to 352 (also while flushing, so a burst cannot exceed the cap by more than a frame's registrations).
- `dyn:map/` and the built-in `dyn:white`, `dyn:black`, `dyn:hitbox` cannot be re-created lazily, so they are never idle-evicted. Maps are bounded by `MAX_MAPS`; when the table is full, the least recently drawn map is removed and its layer freed instead of dropping the new map.
- A failed upload no longer leaves the key marked as registered, so it is retried instead of falling back to missing forever.

## Other allocators checked

See the report in the task list; each allocator is listed with its status.

## Allocators audited

- `EntityRenderer` size-class arrays (entity, hand, block-entity, display scenes): fixed (free list, deferral).
- `BlockEntityScene` skull skins: were never freed or replaced; now freed on replace, idle for 1800 frames, and capped at 128. Wanted-skin requests are capped at 64 per frame. The asset-keyed texture map is bounded by the finite set of asset paths.
- `mcr-ui` sprite atlas (`add_raw`): had no free. `Sprites::free_raw` returns a slot to a per-size free list that `add_raw` reuses. Item icons are capped at 1024 (LRU, never evicting an icon drawn this frame) and freed on a GUI scale change; tab-list heads (256) and server favicons (64) evict entries not used this frame. Mod-manager icons stay one per installed mod. Font glyph pages are bounded by the glyph set.
- `FlatRenderer` textures (portal, beam, ui page mirrors): single replaced slots, dropped by wgpu on replace; pages mirror the UI atlas page count.
- `mcr-render` block atlas array, effect sprite array, `mcr-particles` atlas: built once per resource load and dropped on reload.
- `mcr-modbridge` Java textures: owned by Java handles, dropped with them (not part of this change).

## Display teleport duration

`Display` replaces the generic entity interpolation with `LinearInterpolationHandler(teleport_duration)`: the synced int (index 10, clamped to 0..59) is the number of ticks a position or rotation update takes. With 0 the entity snaps. Otherwise, a new server target starts `remaining = duration`, and each tick the entity covers `1 / remaining` of the way left (position lerp, shortest-path yaw, linear pitch). `PoseLerp` ports this; the entity layer feeds its output into the track as the entity's tick position, so the usual previous/current partial-tick blend renders it.

## Text display draw order

Vanilla submits text as `TranslucentSubmit`s, sorted far to near by the squared distance of the pose's translation. The flat renderer grouped text quads by (kind, texture), so overlapping displays blended in push order. Text display groups now carry a rank: the displays are sorted far to near and each one's background and glyph pages draw together, with see-through and normal displays in one sequence. The remaining difference: vanilla sorts text together with other translucent entity submits (armor glint, translucent models); ours keep their own passes.

## Dialog checkbox

The dialog screen's boolean controls are `Checkbox` widgets in the body widget set, positioned by the same row layout as the other inputs. A toggle plays the click sound; the submitted value reads the widget's state.

## Block markers

`block_marker` is `BlockMarker`: a 0.5 quad of the block's particle sprite, no motion, 80 ticks, full opacity and no tint. It uses its own particle kind in the terrain pool that draws the whole sprite instead of a random quarter. `ClientLevel.getMarkerParticleTarget` picks the block: in creative mode holding a barrier or light, the animate scan emits a marker at the centre of every sampled state of that block. The app's particle texture lookup now falls back to the model's `particle` texture for models without quads (barrier and light have none).

## Campfire food smoke

`CampfireBlockEntity.particleTick`: for each filled slot, 20% per tick, four smoke particles at the slot's corner, rotated by the facing's 2D data value. The animate table keeps a lit campfire's facing and the app passes the filled slots from the block entity data.

## Client ticks

Each is `Block entity ticker -> animate function`:
- Spawner: smoke and flame at one random point of the block while a player is in `RequiredPlayerRange` and a mob is set. The cage spin now stops without one, and the spawn-delay block event resets the delay as in vanilla.
- Trial spawner: particles by `trial_spawner_state` (small flames, flames and smoke, cooldown smoke and the top-face burst; soul fire when ominous) and the ambient sound at 2% per tick while it can spawn. The display mob spins at the state's speed (200 waiting, 1000 active) by the synced `next_mob_spawns_at`.
- Vault: once a second, 2 to 5 `vault_connection` particles per linked player in range, flying from the keyhole to the player; every tick, smoke (and flames with an item shown) at 50%, and the ambient sound.
- Conduit: nautilus particles from above the block toward each frame block (1 in 50 per block per tick) and at the attack target, plus the frame block list that decides activation.

`enchant`, `nautilus` and `vault_connection` are `FlyTowardsPositionParticle`s (start offset by the velocity, flying back to the spawn point over 30 to 39 ticks); before they drifted as generic particles. The entity-event driven vault activation and deactivation bursts are not part of this change.

## Hostile input

The animate scans ignore positions beyond 30 million blocks (so offset arithmetic cannot overflow, `i32::MIN.abs()` included). One budget in the app (2048 particles, 64 sounds per tick) wraps every emission path, including the per-block-entity tickers (at most 1024 per tick). Block entity time math (`game_time` differences, banner phase, beam ends) saturates or wraps.
