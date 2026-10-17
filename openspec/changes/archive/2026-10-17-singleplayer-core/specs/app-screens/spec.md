## MODIFIED Requirements

### Requirement: App flow
The app SHALL start on the title screen, reach the server list from Multiplayer and the world list from Singleplayer, connect through a connecting screen, and show the disconnected screen with the reason when a session ends, returning to the server list. Playing a world SHALL start its local server, join it when ready and return to the world list after saving.

#### Scenario: Disconnect
- **WHEN** the server closes the connection during play
- **THEN** the disconnected screen shows the reason and its button returns to the server list

#### Scenario: Direct connect
- **WHEN** `--connect host` is given
- **THEN** the app joins without showing menus

#### Scenario: Direct singleplayer
- **WHEN** `--singleplayer <folder>` is given
- **THEN** the app starts that world's server and joins it without showing menus
