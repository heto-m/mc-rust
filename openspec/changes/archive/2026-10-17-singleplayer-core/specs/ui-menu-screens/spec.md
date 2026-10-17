## MODIFIED Requirements

### Requirement: Pause screen
The system SHALL show Return to Game, Options and Disconnect buttons returning `Pop`, `OpenOptions` and `Disconnect`, and Escape SHALL close it. For a local server the Disconnect button SHALL read "Save and Quit to Title" and the screen SHALL report that it pauses the game.

#### Scenario: Escape
- **WHEN** Escape is pressed
- **THEN** the screen is popped

#### Scenario: Local server
- **WHEN** the pause screen is opened in singleplayer
- **THEN** it pauses the game and its last button reads "Save and Quit to Title"

### Requirement: Title screen
The system SHALL show the title logo over a plain background with Singleplayer, Multiplayer, Options and Quit buttons. Singleplayer SHALL be enabled when a world store is available and push the world list.

#### Scenario: Multiplayer
- **WHEN** Multiplayer is clicked
- **THEN** the server list screen is pushed

#### Scenario: Singleplayer
- **WHEN** Singleplayer is clicked
- **THEN** the world list screen is pushed
