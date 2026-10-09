## Why

The client cannot join a server until every play-state clientbound packet about the player, inventory, chat, scoreboard, UI and recipes can be decoded and encoded exactly as vanilla 26.3 does.

## What Changes

- Add 58 play clientbound packets: login/respawn/spawn info, player position/abilities/health/experience, chat (system, disguised, signed), player info, containers and merchant offers, titles/boss bars/scoreboard/teams, map data, the Brigadier command graph, recipe displays and recipe book, advancements, stats and misc UI.
- Add the supporting types: command nodes and argument parsers, slot and recipe display registries, advancement display info, chat signature types.
- Out of scope: text components, item stacks, particles (shared placeholders owned elsewhere), serverbound packets, packet dispatch tables.

## Capabilities

### New Capabilities
- `protocol-play-player`: wire format of the player-facing play clientbound packets.

### Modified Capabilities
