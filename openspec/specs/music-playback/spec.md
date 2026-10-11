# music-playback Specification

## Purpose
Situational music selection, song scheduling, fades and jukebox suspension.

## Requirements

### Requirement: Situational music selection
The system SHALL choose the wanted music from the current situation: menu music without a world, the end-boss track in the End while a boss bar asks for music, otherwise the active background-music attribute's underwater, creative or default track in that order of preference.

#### Scenario: Menu
- **WHEN** no world is joined
- **THEN** the menu music (20 to 600 tick delay, replacing the current track) is wanted

#### Scenario: Creative flight
- **WHEN** the player is in creative mode and may fly, and the attribute has a creative track
- **THEN** the creative track is wanted

#### Scenario: Dragon fight
- **WHEN** the dimension is the End and a boss bar has the play-music flag
- **THEN** the dragon track is wanted

#### Scenario: No music
- **WHEN** the active attribute has no applicable track
- **THEN** nothing is wanted and the song countdown is held at 100 ticks or more

### Requirement: Song scheduling
The music manager SHALL wait an initial 100 ticks, start the wanted track when its countdown reaches zero (never while a level is loading), then wait a random delay between the track's minimum and maximum, bounded by the music frequency option, after it ends.

#### Scenario: Replacement
- **WHEN** a replacing track becomes wanted while another plays
- **THEN** the playing track stops and the new one starts after a delay of at most half the new minimum

#### Scenario: Constant frequency
- **WHEN** the music frequency is constant
- **THEN** the next song follows 100 ticks after the previous one ends

### Requirement: Music volume fade
The manager SHALL move its gain toward the music-volume attribute, rising slowly and falling exponentially, and stop the track when the gain reaches zero.

#### Scenario: Silent volume
- **WHEN** the music volume attribute becomes zero
- **THEN** the track stops and the next song waits its frequency delay

### Requirement: Jukebox
Music SHALL stop and stay stopped while a jukebox record plays within 64 blocks of the listener.

#### Scenario: Record starts
- **WHEN** a jukebox starts playing nearby while music plays
- **THEN** the music stops and no song starts until the record ends or is out of range

### Requirement: Hostile music data
Music delays from registry data SHALL be clamped to 24 hours of ticks, and unplayable sounds SHALL be skipped without retrying every tick.

#### Scenario: Huge delay
- **WHEN** a biome declares a maximum delay of 2000000000
- **THEN** it is treated as 1728000
