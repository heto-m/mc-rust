# sound-events Specification

## Purpose
Models vanilla sounds.json events and picks weighted variants deterministically from a seed.

## Requirements

### Requirement: sounds.json model
The system SHALL parse `sounds.json` into events whose entries are either sound files or `type: event` references, with `weight`, `volume`, `pitch`, `stream`, `attenuation_distance` and `preload` fields defaulting to 1, 1, 1, false, 16 and false.

#### Scenario: Defaults
- **WHEN** an entry is the bare string `"block/stone/step1"`
- **THEN** it has weight 1, volume 1, pitch 1, attenuation distance 16 and is not streamed

#### Scenario: Event reference
- **WHEN** an entry has `"type": "event"`
- **THEN** selection resolves it by picking from the referenced event, scaling volume and pitch by the reference's values

### Requirement: Seeded weighted selection
`SoundManager::play` SHALL pick a variant by weight using an RNG seeded with the caller's seed, so equal seeds give equal choices.

#### Scenario: Determinism
- **WHEN** the same event is picked twice with the same seed
- **THEN** the same variant is chosen

#### Scenario: Weights
- **WHEN** a variant has weight 3 and another weight 1, over many seeds
- **THEN** the first is chosen about three times as often

#### Scenario: Unknown event
- **WHEN** the event id is not defined
- **THEN** nothing is played and no error is raised

### Requirement: Client-side sounds
The client SHALL play the sounds vanilla plays locally: lightning thunder and impact, the portal trigger,
the user's own eating, drinking, bucket, flint and steel and spyglass sounds, air bubble pops and bubble
column inside sounds. `stop_sound` SHALL stop matching sounds, and entity-bound sounds SHALL follow their
entity.

#### Scenario: Stop sound
- **WHEN** the server sends stop_sound for the music source
- **THEN** the music stops

### Requirement: Riding loops and dash sounds
Riding a happy ghast or a nautilus SHALL play its riding loop, and the nautilus SHALL play its dash
sounds.

#### Scenario: Happy ghast
- **WHEN** the player mounts a happy ghast
- **THEN** the riding loop plays until they dismount

### Requirement: Distance-delayed level sounds
Level events that vanilla plays with a distance delay SHALL arrive later the farther away they are.

#### Scenario: Far vault
- **WHEN** a vault 64 blocks away activates
- **THEN** its sound plays after the delay for that distance

### Requirement: Stop sound in mod mode
In mod mode `stop_sound` SHALL reach the sound engine that plays the sounds it targets.

#### Scenario: Stop music with mods
- **WHEN** the server sends stop_sound for music in mod mode
- **THEN** the music Java plays stops
