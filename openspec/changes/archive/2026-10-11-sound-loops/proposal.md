# Proposal

## Why

The earlier ambient-audio change left the client-started entity loops, creaking heart idle sounds, credits
music and the now-playing toast out of scope. Without them elytra flight, bees and guardian beams are
silent, the End poem has no screen or music, and the music-toast option does nothing.

## What Changes

- Add elytra gliding wind, per-bee flying/aggressive loops (nearest 12 only) and guardian beam charge loops.
- Add creaking heart idle sounds as a block ambience cue (the rest of the heart's sounds are server-sent).
- Add the End poem / credits screen for game event 4 with `music.credits` playing while it is open; Esc or the
  end of the scroll sends perform-respawn.
- Add the now-playing music toast and its fixed form on the pause screen, driven by the `musicToast` option.
- Minecart loops were already done by ambient-audio and are unchanged.

Out of scope: end-portal shader backdrop, credits vignette and logo, key-release based speed-up (the UI has no
key-release events; Space toggles, Up/Down set the direction).

## Capabilities

### New Capabilities
- `entity-sound-loops`: elytra, bee and guardian beam loops.
- `credits-screen`: the poem and credits screen lifecycle.

### Modified Capabilities
- `ambient-sounds`: creaking heart idle cue.
- `music-playback`: credits music, track start/stop notifications.
- `toasts`: now-playing toast.

## Impact

`mcr-audio` (`loops`, block cue), `mcr-client` (game event 4), `mcr-ui` (credits screen, toast, screen
hooks), `mcr-app` (`entity_loops`, `play`, `audio`, `ambience`).
