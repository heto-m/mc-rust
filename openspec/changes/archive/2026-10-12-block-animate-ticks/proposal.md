# Proposal

## Why

mcr renders the particles the server sends, but not the ones vanilla's client makes by itself: torch flames and smoke,
campfire columns, lava pops, drips, falling leaves, portal swirls and the nether biomes' ash and spores. In mod mode the
Java copies of these are dropped on purpose, so they are missing there too. The only part of `animateTick` mcr runs is
the sound half, in a separate scan that duplicates the sampling.

## What Changes

- One scan per tick, as vanilla's `ClientLevel.animateTick`: 667 random positions in radius 16 and 667 in radius 32 around
  the player's block, each running the block's, the fluid's and the biome's ambient effects.
- A new crate `mcr-animate` holds the whole `animateTick` behaviour of every vanilla 26.3 block that has one, as flat
  per-block-state lookup tables built once, and emits particles and positional sounds through a sink trait.
- Biome ambient particles come from the synced `minecraft:visual/ambient_particles` attribute (dimension type list
  followed by the biome's appended entries); the dripstone default drip comes from `minecraft:visual/default_dripstone_particle`.
- Particles honour the Particles option (all, decreased, minimal), the 32 block distance limit, the always-visible and
  limiter-overriding particle types, as vanilla's `ClientLevel.addParticle`.
- The sound half moves out of `mcr-audio`'s `block_ambience` into the same scan; mod mode (Java owns sounds) takes the
  particles only.
- The campfire block entity's rising smoke is spawned from the loaded campfires.

Out of scope: block entity effects beyond campfires (conduit, spawners, vaults, campfire food smoke), barrier and light
marker particles, and particle simulation itself. The end gateway counts exposed faces by neighbouring sturdy faces.
