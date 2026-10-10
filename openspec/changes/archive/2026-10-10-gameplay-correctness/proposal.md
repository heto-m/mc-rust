## Why

client-core joins and moves, but gameplay falls back to server resyncs: container clicks send unhashed
components, digging uses an empty-hand estimate, item use is a bare packet, vehicles and edit screens are
unsupported, and much server state is dropped.

## What Changes

- Vanilla CRC32C component hashing and full client-side container click prediction for every click type.
- Exact destroy-progress digging (tool rules from server tags, enchantments, effects, attributes) with
  block prediction sequence ids and rollback on `block_changed_ack`.
- Item use in vanilla order (`useItemOn` then `useItem`), release actions, cooldowns, offhand.
- Vehicle riding: mount/dismount, `player_input`, `move_vehicle`, `paddle_boat`, boat and horse simulation.
- Sign, book, anvil rename, trade selection, beacon and enchant button actions.
- Stored recipe book, advancements, map data, merchant offers, cooldowns, stats, waypoints; `transfer`.
- Effects and attributes feed physics; extended live physics validation.

## Capabilities

### New Capabilities
- `gameplay-correctness`: client-side gameplay behaviour that must match vanilla so the server never corrects the client.

### Modified Capabilities

## Impact

`mcr-client` (inventory, actions, handler, state, new modules), `mcr-physics` (vehicles, effects).
