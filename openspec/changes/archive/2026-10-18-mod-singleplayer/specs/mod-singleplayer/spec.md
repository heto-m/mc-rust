## ADDED Requirements

### Requirement: Java world screens over mcr's saves
In mod mode Java's Singleplayer button and every other path to its world list SHALL open Java's own world list, reading mcr's `<game dir>/saves`. Playing a world SHALL start mcr's managed server for it and join through the net bridge; Java's integrated server SHALL never start.

#### Scenario: Play from Java's list
- **WHEN** the user selects a world in Java's world list and clicks Play Selected World
- **THEN** mcr starts that world's server, Java shows mcr's progress, and the player joins the world

#### Scenario: Integrated server blocked
- **WHEN** a mod calls `Minecraft.doWorldLoad`
- **THEN** no integrated server starts and Java returns to its title screen

### Requirement: Java create options mapped
Java's Create New World SHALL create the world through mcr with its name, game mode, difficulty, allow commands, world type, seed, structures and bonus chest. A world type or mode mcr cannot create SHALL be refused on Java's screen with a toast; data packs and experiments SHALL be reported as not applied.

#### Scenario: Create a creative superflat world
- **WHEN** the user creates a creative superflat world named "Flat" in Java
- **THEN** mcr creates the folder, its server generates a flat world, and the player joins in creative

#### Scenario: Unsupported type
- **WHEN** the user picks the single biome type and clicks Create
- **THEN** Java stays on the create screen with a toast and no folder is created

### Requirement: Java never writes an open world
While mcr's local world is preparing, running or stopping, Java SHALL refuse to open any world folder, so it never writes `level.dat` or takes `session.lock` beside mcr's server.

#### Scenario: Edit during play
- **WHEN** a mod asks Java's level storage to open a world while mcr's server runs
- **THEN** the access fails with an error and the world files are untouched

### Requirement: Progress, EULA and failure in Java
For a world started from Java, mcr SHALL show no native screens; Java SHALL show mcr's status lines with a Cancel button, ask for the EULA once, and show failures with a button back to the world list.

#### Scenario: First world
- **WHEN** the EULA was never accepted and the user plays a world from Java
- **THEN** Java asks for the EULA, and the world starts after "I agree"

#### Scenario: Server fails to start
- **WHEN** the server cannot be prepared or exits before it is ready
- **THEN** Java shows the reason and its button returns to the world list

### Requirement: Pause in mod mode
In a local world with Java's GUI, mcr SHALL send `tick freeze` while Java's GUI pauses (vanilla `Gui.isPausing()`) and `tick unfreeze` when it stops; Java's client SHALL pause like vanilla singleplayer. Open to LAN SHALL never be offered.

#### Scenario: Pause menu
- **WHEN** the player presses Esc in a local world and then clicks Back to Game
- **THEN** the server console receives `tick freeze` and then `tick unfreeze`

### Requirement: Save and quit from Java
Java's pause screen SHALL offer Save and Quit to Title in a local world. Leaving SHALL stop the server through mcr's save and stop path while Java shows a saving screen until the server exited, then Java's title screen.

#### Scenario: Save and quit
- **WHEN** the player clicks Save and Quit to Title
- **THEN** the server saves and stops, Java shows Saving world until then, and the world can be opened again

### Requirement: Local connection loss
When the local server stops unrequested or the connection to it is lost, Java SHALL show its disconnected screen whose button returns to the world list, and mcr SHALL stop the server.

#### Scenario: Server crash
- **WHEN** the server process dies while the player is in the world
- **THEN** Java shows the disconnected screen and Back leads to the world list
