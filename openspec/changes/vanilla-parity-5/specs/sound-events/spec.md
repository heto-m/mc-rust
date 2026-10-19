## ADDED Requirements

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
