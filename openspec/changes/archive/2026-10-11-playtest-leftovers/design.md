## Decisions

- **One underwater signal.** The old app check used `amount / 9` as the surface height of every water block, so the top ninth of a block with water above read as dry. Any eye passing through that band (sinking, swimming up or down, the camera's eye-height smoothing on pose change) emitted exit+enter. The physics already computes vanilla's eye-in-fluid per tick, so the ambience reads it instead of re-deriving it.
- **Passengers.** The rider's physics skips `tick`, which would freeze `eye_in_water` while riding. `update_passenger_fluid` runs the same fluid update after the rider is placed on its seat; the velocity it touches is reset by the next seat snap.
- **Camera fog** stays a camera-position check (vanilla `getFluidInCamera`) but shares the height rule with physics; the fog no longer also triggers on a non-zero water-vision counter.
- **Hurt sounds only for the local player.** Vanilla's client `handleDamageEvent` calls `playSound`, which is a no-op on the client for every entity except the local player; everyone else's hurt and fall sounds come from the server.
