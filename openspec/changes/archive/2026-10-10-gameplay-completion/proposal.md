## Why

`gameplay-correctness` left gaps: many data components hash to 0, shift-click routing is only predicted for
a few menus, crafting-result pickups and several rideables and elytra flight are not simulated, so the server
corrects the client in those cases.

## What Changes

- CRC32C `HashOps` encoders for every persistent 26.3 data component, text hover/click events, nbt and object text.
- `quickMoveStack` routing for every menu type and crafting-result slot pickups predicted from the server's result slot.
- Riding: minecart, strider, camel, pig, happy ghast and llama caravan behaviour matching the vanilla client.
- Elytra: fall-flying start, glide physics, firework boost, landing; riptide trident launch.
- Live tests per area with zero server corrections.

Out of scope: mounts the vanilla client does not predict (the server position is followed), singleplayer.

## Capabilities

### New Capabilities

### Modified Capabilities
- `gameplay-correctness`: complete component hashing, per-menu quick-move and result prediction, rideables and elytra.

## Impact

`crates/mcr-client` (hashing, menu, riding, player), `crates/mcr-physics` (fall flying, riptide).
