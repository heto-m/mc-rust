# Design

## Environment attributes

26.3 moved biome music and ambience into `attributes` (`minecraft:audio/background_music`,
`audio/ambient_sounds`, `audio/music_volume`, `audio/firefly_bush_sounds`) on biome and dimension-type
registry entries. The biome value overrides the dimension's per attribute. Values are accepted bare or
wrapped as `{argument, modifier}`; other modifiers are treated as override.

## Split

Pure rules (music state machine, mood counter, fades, step cadence, cues) live in `mcr-audio` and are unit
tested. `mcr-app` samples the client state (biome at the player, light, blocks, fluids, tags) and feeds them in.

## Local player only

The server sends step, swim, splash and fall sounds for every other entity, and excludes the sound's source
player, so only the local player's movement sounds are predicted. Landing sounds need the fall distance, which
the physics resets in the landing tick, so a tracker records the airborne peak.

## Block ambience

A per-state table is built once from block names and properties. Each tick samples 667 random positions
within 16 and 32 blocks of the player like vanilla and rolls the matching cue.

## Hostile data

Tick delays, search extents, offsets, chances and music delays are clamped; at most 8 additions; records
and minecart voices are capped.
