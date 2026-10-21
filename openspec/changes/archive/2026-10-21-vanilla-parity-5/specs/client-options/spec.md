## ADDED Requirements

### Requirement: World Options in the pause menu
The pause menu SHALL show Options and World Options, and World Options SHALL hold the world's difficulty,
game rules, game modes, commands and multiplayer settings.

#### Scenario: Local world
- **WHEN** the player pauses in a local world
- **THEN** World Options opens a screen with Open to LAN in its multiplayer section

### Requirement: Pack screen icons and drops
The pack screen SHALL show each pack's icon and accept dropped pack files.

#### Scenario: Drop a pack
- **WHEN** a pack zip is dropped on the pack screen
- **THEN** it is copied into the packs folder and listed

### Requirement: Complete debug chords
All vanilla debug chords SHALL work, including holding F3+C, F3+F6, F3+S, F3+L, F3+1 and F3+4, and
chords SHALL fire while a screen that does not consume the key is open.

#### Scenario: Lightmap
- **WHEN** the player presses F3+4
- **THEN** the lightmap texture view toggles
