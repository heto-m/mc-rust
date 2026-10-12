# Design

## Scan

`Animator::tick(center, world, sink)` draws 667 samples per radius with a SplitMix generator. Each sample reads the block
state once and indexes a flat `Vec<Info>` (one entry per block state, built from `mcr-data` once): behaviour kind, a small
argument, static shape flags, fluid class and block id. States without behaviour cost one load. Per sample the work is
bounded: at most one behaviour, one fluid effect, and the biome ambient list (at most 16 entries). A per-tick budget caps
the particles (2048) and sounds (64) the scan may emit, so hostile registry data (probabilities, list lengths) cannot
flood the pools.

## Tags

Rules that vanilla expresses with block tags (impermeable, desert triggers, pale oak logs, enchantment power providers,
the dried ghast trigger set, the poplar leaf ambience set) read a per-block bit set rebuilt when the synced tag lists
change. Flammability for fire smoke is a name list of the vanilla bootstrap.

## Sink

`Sink::particle(&Particle, pos, vel, always)` and `Sink::sound(event, pos, volume, pitch, source)`. The app adapter maps
particles to `WorldEffects::add_particle_vanilla`, which reproduces `calculateParticleLevel` (decreased drops a third,
minimal drops all but the always-visible tenth) and the 32 block limit, and sounds to `Audio::play`.

## Mod mode

`ClientLevelMixin` already drops Java's particles during `animateTick`; Java keeps playing its own sounds. The scan runs
with `sounds: false` while Java owns audio.

## Replaced

`mcr-audio::block_ambience` and `Ambience::animate_blocks` are removed; their cues and tests live in `mcr-animate`.
