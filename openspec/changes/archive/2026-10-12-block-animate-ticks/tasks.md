## 1. Crate

- [x] 1.1 `mcr-animate` skeleton: `Animator`, `AnimateWorld`, `Sink`, seeded sampling of 667 positions per radius, per-tick budget
- [x] 1.2 Flat per-state `Info` table from `mcr-data`, tag bit sets, flammable names
- [x] 1.3 Visual attributes: ambient particle list, default dripstone particle, firefly sounds, water evaporates, parsed from registry NBT with hostile-input bounds

## 2. Effects

- [x] 2.1 Fire family: torches, redstone torches, fire, candles, campfires, furnaces, brewing stand, with seeded tests
- [x] 2.2 Portal family: nether portal, end portal, ender chest, respawn anchor, reverse portal, end rod, enchanting table, with tests
- [x] 2.3 Redstone family: ore, wire, repeater, lever, sculk sensor, lightning rod, with tests
- [x] 2.4 Drip family: lava and water drips, beehive, wet sponge, crying obsidian, pointed dripstone, leaves in rain, with tests
- [x] 2.5 Foliage family: leaves, falling blocks, spore blossom, mycelium, wither rose, firefly bush, with tests
- [x] 2.6 Sound-only and misc: bubble column, creaking heart, dried ghast, eyeblossom, hanging moss, potent sulfur, sand, dry vegetation, fluid ambience
- [x] 2.7 Biome ambient particles with tests
- [x] 2.8 Bounded-cost test and a criterion bench

## 3. App

- [x] 3.1 `WorldEffects::add_particle_vanilla` with vanilla `calculateParticleLevel` and limiter rules
- [x] 3.2 Driver in `mcr-app`: world adapter, registry visuals, sink to effects and audio, mod mode particles only
- [x] 3.3 Remove `block_ambience` and `Ambience::animate_blocks`
- [x] 3.4 Campfire block entity smoke

## 4. Finish

- [x] 4.1 Merge main, full gate: fmt, clippy, test
- [x] 4.2 `openspec validate block-animate-ticks --strict`, archive, no TBD Purpose
