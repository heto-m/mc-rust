# mod-sound-bridge Specification

## Purpose
Sound for the embedded client: Java's `SoundEngine` logic stays, its OpenAL layer is replaced by mixins that send
channel commands, PCM uploads and pushed streams to `mcr-audio` over FFM, and in mod mode every sound is played by
exactly one side (mcr owns its own predictions and the local player's movement; Java owns everything the server
sends plus music, ambience, GUI and mod sounds).

## Requirements

### Requirement: Java sound through mcr-audio
In mod mode, Java's `SoundEngine` SHALL run normally and its channel operations SHALL be executed by mcr-audio instead of OpenAL. No OpenAL device SHALL be opened.

#### Scenario: Mod sound plays
- **WHEN** a test mod plays its own sound event backed by an OGG file in its jar
- **THEN** mcr-audio plays one voice whose PCM correlates with the file's decoded audio at 0.95 or more

#### Scenario: No OpenAL
- **WHEN** Java has played sounds for 60 seconds
- **THEN** no OpenAL device was opened in the process

### Requirement: Streams and buffers
Static sound buffers SHALL be uploaded to Rust once per buffer and reused. Streamed sounds SHALL be fed as decoded PCM from Java's audio stream without gaps under normal load.

#### Scenario: Music stream
- **WHEN** Java's music manager starts a track on the title screen
- **THEN** mcr-audio plays it as a pushed stream with zero underruns over 30 seconds

### Requirement: Channel state queries
`Channel.playing()` and `stopped()` SHALL reflect mcr-audio's voice state without a downcall.

#### Scenario: Sound ends
- **WHEN** a non-looping static sound finishes in the mixer
- **THEN** Java's channel reports stopped by the next sound tick and the channel returns to its pool

### Requirement: Single playback in mod mode
Each sound SHALL be played by exactly one side. Java SHALL own server-sent sounds, music, ambience, GUI and mod sounds. mcr SHALL own local-player movement sounds and its predicted interaction sounds.

#### Scenario: Server sound
- **WHEN** the server sends a sound packet for a cow's ambient sound
- **THEN** exactly one voice starts for it

### Requirement: One volume source
Java's sound option sliders SHALL control the volume of both Java-played and mcr-played sounds in mod mode.

#### Scenario: Mute blocks
- **WHEN** the user sets the Blocks slider to 0 in Java's options
- **THEN** mcr-predicted block break sounds are silent
