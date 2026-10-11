# Proposal

## Why

The client only plays sounds the server sends. Vanilla also synthesizes music, biome ambience, block
ambience, weather and the local player's own movement sounds on the client, so a joined world is silent
without them.

## What Changes

- Add `MusicManager` with vanilla situational music, delays, fade and stop rules, and the `music` volume and frequency options.
- Add biome and dimension audio attributes (background music, ambient loop, mood, additions) parsed from the synced registries with hostile-input clamps.
- Add underwater enter/exit, loop and additions sounds.
- Add client-side `animateTick` sounds for lit and ambient blocks and fluids near the player.
- Add footsteps, swim, splash and landing sounds for the local player, and minecart rolling sounds.
- Add rain sounds from the weather ticker and jukebox playback that suspends music.
- Report finished voices from the mixer so tracks and sounds can be tracked.
- Optional `MCR_SOUND_LOG=<file>` writes every started sound for verification.

Out of scope: creaking heart idle sounds, elytra and bee loops, distance-delayed sounds, credits screen
music, now-playing toast.

## Capabilities

### New Capabilities
- `music-playback`: situational music selection and the manager state machine.
- `ambient-sounds`: biome, underwater, block, fluid, weather and minecart ambience.
- `movement-sounds`: local player footsteps, swimming, splash and landing sounds.

### Modified Capabilities
- `audio-mixer`: voices report when they end by themselves.

## Impact

`mcr-audio` (new pure-logic modules, `mcr-nbt` dependency), `mcr-app` (`audio`, `ambience`,
`movement_sounds`, `play`, `shell`, `effects`). No protocol changes.
