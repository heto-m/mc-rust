## Context

Same prediction contract as `gameplay-correctness`: any divergence from vanilla costs a resync or a teleport.

## Decisions

- Hashing: one encoder per persistent component codec. Registry holders hash as their registry name,
  inline holders through the direct codec, holder sets as `#tag` or a compact list. Components that travel
  as NBT (map decorations, lock, loot, debug stick) hash through the NBT tree.
  Non-persistent components (`creative_slot_lock`, `additional_trade_cost`, `map_post_processing`) cannot
  be hashed by vanilla either and are exempt.
- Quick move: each menu kind has a routing function mirroring `quickMoveStack`; fuel, ingredient and
  trade rules come from the synced item components and tags, not hard-coded item lists.
- Crafting results: after a result-slot pickup the ingredient slots are decremented per the recipe grid
  and the result slot is left for the server to refill.
- Riding: the vanilla client only simulates entities that implement local control (boats, horses,
  striders, camels, pigs, happy ghast); minecarts and caravans follow server positions.
- Elytra: fall-flying state is entity flag 7; start and stop go through `player_command`.

## Risks

Anvil and merchant result pickups, bundle interactions and the loom, cartography and smithing result
contents stay with the server (the click is left unpredicted or the server sends the result slot).
Nautilus mounts, minecart physics and llama caravans are not simulated (the player follows the server).

Component predicates inside adventure-mode block predicates are hashed from their NBT form, which differs
from vanilla for boolean fields.
