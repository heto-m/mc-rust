# Proposal

## Why

Java has no audio in mod mode (`mod-fabric-boot` keeps `SoundEngine` unloaded), so mod sounds, Java-side ambience,
music and GUI clicks are silent. At the same time, server sounds would play twice once Java plays them too. Java's
`SoundEngine` logic should stay, because mods (Fabric sound-api, AmbientSounds, Presence Footsteps) hook it. Only
its OpenAL layer is replaced, with `mcr-audio` playing the result.

## What Changes

- The OpenAL layer in `com.mojang.blaze3d.audio` is bridged by mixins:
  - `Library.init` opens no device and builds the channel pools.
  - `Channel.create` returns channels with bridge voice ids.
  - Every `Channel` operation (play, pause, unpause, stop, position, pitch, volume, looping, attenuation,
    relative, attach static buffer or stream, update stream, destroy) becomes a command on a dedicated `mcr-audio`
    command queue.
  - `playing()`/`stopped()` read a shared voice-state table.
  - `SoundBuffer` PCM is uploaded once per buffer. Streams push decoded PCM into a per-voice ring from
    `updateStream`.
  - `Library.createDeviceTracker` reports a single fixed device.
- `mcr-audio` gains:
  - a pushed-PCM source;
  - per-voice pause/resume;
  - a pre-mixed gain mode (Java's `SoundEngine.calculateVolume` already includes category and master);
  - a second command producer;
  - a voice-state table.
- Ownership in mod mode: Java plays server-sent sounds, music (`MusicManager`), ambience, GUI and mod sounds. mcr
  keeps only the sounds of things Java does not simulate: local-player movement and mcr-predicted interactions
  (block hit, break, place). mcr's listener stays authoritative, and Java's `Listener.setTransform` is ignored.
- Java's sound options drive volumes for both sides.

Out of scope:
- Mods calling OpenAL directly (Sound Physics Remastered, Simple Voice Chat's own playback). These are listed in
  the mod manager.
- HRTF.
- Device selection UI.
- Subtitles on mcr-owned sounds.

Acceptance test (mcr-audio offline mixer capture, local vanilla server, Fabric API plus a test mod with its own
`sounds.json` and OGG tone):
- `/playsound minecraft:entity.cow.ambient master @p` starts exactly one voice.
- The test mod's sound plays, and its captured PCM correlates with the tone (≥ 0.95).
- Title-screen music starts as a streamed voice.
- `/stopsound @p` stops all of them.
- No OpenAL device is opened.

## Capabilities

### New Capabilities
- `mod-sound-bridge`: Java sound output through mcr-audio and mod-mode sound ownership.

### Modified Capabilities
- `audio-mixer`: pushed-PCM sources, pause/resume, pre-mixed gain, second producer, voice-state table.

## Impact

`mcr-audio`, `mcr-modbridge` (`sound/`), `mcr-app` (mod-mode sound ownership), `java/mcr-bridge` (mixins).
