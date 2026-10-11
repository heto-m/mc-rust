## Why

Owner playtests left four loose ends: hurt sounds that ignore the damage type, underwater enter/exit sounds that flicker while sinking or swimming near a pool bottom, a pickup animation that was reported missing, and the newly merged water top faces that had not been looked at on screen.

## What Changes

- Hurt sounds: verified that `damage_event` resolves the damage type's `effects` from the synced `damage_type` registry (falling back to vanilla's own types by name for known-pack entries without data) and that the local player plays `player.hurt`, `hurt_on_fire`, `hurt_drown`, `hurt_sweet_berry_bush` or `hurt_freeze`. Other entities' hurt and fall sounds arrive as server sound packets, as in vanilla. Adds handler-level tests.
- Underwater: ambience, music, minecart variants and the water-vision counter use the physics `eye_in_water` state (vanilla `isEyeInFluid(WATER)`), not a separate block check that ignored the "same fluid above" full height. Passengers refresh their fluid contact every tick, as vanilla's passenger tick does. The camera fog check uses vanilla `getHeightForCamera` through a shared physics function.
- Pickup: verified the `take_item_entity` path against vanilla `ItemPickupParticle` (3 ticks, squared-time lerp towards the collector's mid-body, target lerped by partial tick, entity removed when its stack empties). Pickups are now dropped with the entities on a world rebuild or reconfiguration. Adds handler-level tests.
- Water merge: rendered a flat vanilla ocean with the world viewer's screenshot mode and inspected it.

## Impact

- Affected specs: `ambient-sounds`, `client-state`, `player-physics`.
- Code: `mcr-physics` (fluid), `mcr-client` (riding, state, tests), `mcr-app` (ambience, effects, play).
