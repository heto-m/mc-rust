## MODIFIED Requirements

### Requirement: Java create options mapped
Java's Create New World SHALL create the world through mcr with its name, game mode, difficulty, allow commands, world type, seed, structures, bonus chest, changed game rules, enabled experiments, enabled data packs from its pack folder and custom superflat settings. A world type or mode mcr cannot create (single biome, debug) SHALL be refused on Java's screen with a toast and no folder; enabled packs mcr cannot load (mod packs) SHALL be reported as not applied. Java SHALL keep its temporary pack folder until the session is ready, failed or stopped.

#### Scenario: Create a creative superflat world
- **WHEN** the user creates a creative superflat world named "Flat" in Java
- **THEN** mcr creates the folder, its server generates a flat world, and the player joins in creative

#### Scenario: Unsupported type
- **WHEN** the user picks the single biome type and clicks Create
- **THEN** Java stays on the create screen with a toast and no folder is created

#### Scenario: Game rule from Java
- **WHEN** the user turns off daylight cycle in Java's game rules and creates the world
- **THEN** the server console receives `gamerule advance_time false` before the player joins

### Requirement: Pause in mod mode
In a local world with Java's GUI, mcr SHALL send `tick freeze` while Java's GUI pauses (vanilla `Gui.isPausing()`) and `tick unfreeze` when it stops; Java's client SHALL pause like vanilla singleplayer. Once the world is open to LAN, neither SHALL pause.

#### Scenario: Pause menu
- **WHEN** the player presses Esc in a local world and then clicks Back to Game
- **THEN** the server console receives `tick freeze` and then `tick unfreeze`

### Requirement: Local connection loss
When the local server stops unrequested or the connection to it is lost, Java SHALL show a disconnected screen with mcr's reason ("The local server stopped" and the server's last lines) whose button reads Back to World List and returns to the world list, and mcr SHALL stop the server.

#### Scenario: Server crash
- **WHEN** the server process dies while the player is in the world
- **THEN** Java shows "The local server stopped" with Back to World List, which leads to the world list

## ADDED Requirements

### Requirement: Local labels on Java's warning screen
In a local world, Java's custom-dialog warning screen SHALL label its leave button "Save and Quit to Title" like the pause screen.

#### Scenario: Custom dialog warning
- **WHEN** a server dialog's warning screen opens in a local world
- **THEN** its leave button reads Save and Quit to Title

### Requirement: Fresh levels from mods
In mod mode `WorldOpenFlows.createFreshLevel` SHALL be refused with a toast before Java creates any folder.

#### Scenario: Mod creates a level
- **WHEN** a mod calls `createFreshLevel("ModWorld", …)`
- **THEN** no `ModWorld` folder exists in the saves directory and Java shows the screen the mod passed
