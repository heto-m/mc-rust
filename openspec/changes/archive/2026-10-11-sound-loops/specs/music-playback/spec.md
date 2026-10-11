## ADDED Requirements

### Requirement: Credits music
While the credits screen is open the music manager SHALL want `music.credits`, replacing the current track, and SHALL drop it when the screen closes.

#### Scenario: Credits open
- **WHEN** the credits screen opens during another track
- **THEN** `music.credits` replaces it

#### Scenario: Credits close
- **WHEN** the screen closes
- **THEN** the credits track stops and the normal delay applies

### Requirement: Track notifications
The audio layer SHALL report when a music track starts, with the language key of the picked file (path separators as dots), and when it stops or ends.

#### Scenario: Track starts
- **WHEN** `music/game/clark` starts
- **THEN** the key `music.game.clark` is reported
