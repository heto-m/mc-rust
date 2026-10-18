# mod-singleplayer Specification

## Purpose
In mod mode, Java's own world list, create and pause screens drive mcr's managed vanilla server, so modded singleplayer uses the same saves, pause and save-and-quit flow as native mcr.

## Requirements

### Requirement: Java world screens over mcr's saves
In mod mode Java's Singleplayer button and every other path to its world list SHALL open Java's own world list, reading mcr's `<game dir>/saves`. Playing a world SHALL start mcr's managed server for it and join through the net bridge; Java's integrated server SHALL never start.

#### Scenario: Play from Java's list
- **WHEN** the user selects a world in Java's world list and clicks Play Selected World
- **THEN** mcr starts that world's server, Java shows mcr's progress, and the player joins the world

#### Scenario: Integrated server blocked
- **WHEN** a mod calls `Minecraft.doWorldLoad`
- **THEN** no integrated server starts and Java returns to its title screen

### Requirement: Java create options mapped
Java's Create New World SHALL create the world through mcr with its name, game mode, difficulty, allow commands, world type, seed, structures, bonus chest, changed game rules, enabled experiments, enabled data packs from its pack folder and custom superflat settings. A world type or mode mcr cannot create (single biome, debug) SHALL be refused on Java's screen with a toast and no folder.

#### Scenario: Create a creative superflat world
- **WHEN** the user creates a creative superflat world named "Flat" in Java
- **THEN** mcr creates the folder, its server generates a flat world, and the player joins in creative

#### Scenario: Unsupported type
- **WHEN** the user picks the single biome type and clicks Create
- **THEN** Java stays on the create screen with a toast and no folder is created

#### Scenario: Game rule from Java
- **WHEN** the user turns off daylight cycle in Java's game rules and creates the world
- **THEN** the server console receives `gamerule advance_time false` before the player joins

### Requirement: Java data pack handover
Enabled packs mcr cannot load (mod packs) SHALL be reported with a toast as not applied. Java SHALL keep its temporary pack folder, which mcr copies from, until the session is ready, failed or stopped, and then delete it.

#### Scenario: Pack dropped onto Java's Data Packs screen
- **WHEN** the user drops `tools.zip` onto Java's Data Packs screen, enables it and creates the world
- **THEN** mcr copies it into `<world>/datapacks` and Java deletes its temporary folder once the world is ready

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
In a local world with Java's GUI, mcr SHALL send `tick freeze` while Java's GUI pauses (vanilla `Gui.isPausing()`) and `tick unfreeze` when it stops; Java's client SHALL pause like vanilla singleplayer. Once the world is open to LAN, neither SHALL pause.

#### Scenario: Pause menu
- **WHEN** the player presses Esc in a local world and then clicks Back to Game
- **THEN** the server console receives `tick freeze` and then `tick unfreeze`

### Requirement: Save and quit from Java
Java's pause screen SHALL offer Save and Quit to Title in a local world. Leaving SHALL stop the server through mcr's save and stop path while Java shows a saving screen until the server exited, then Java's title screen.

#### Scenario: Save and quit
- **WHEN** the player clicks Save and Quit to Title
- **THEN** the server saves and stops, Java shows Saving world until then, and the world can be opened again

### Requirement: Local connection loss
When the local server stops unrequested or the connection to it is lost, Java SHALL show a disconnected screen with mcr's reason ("The local server stopped" and the server's last lines) whose button reads Back to World List and returns to the world list, and mcr SHALL stop the server.

#### Scenario: Server crash
- **WHEN** the server process dies while the player is in the world
- **THEN** Java shows "The local server stopped" with Back to World List, which leads to the world list

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

### Requirement: Difficulty in Java's World Options
In a local mcr world, Java's World Options SHALL let the owner change and lock the difficulty as vanilla singleplayer does. A change SHALL reach mcr's managed server; a lock SHALL hold in both clients for the rest of the session and be stored in the world's `level.dat` once its server stopped. Hardcore worlds and locked worlds SHALL keep the difficulty controls inactive, and Rust SHALL refuse changes for them.

#### Scenario: Change difficulty
- **WHEN** the owner of a local world without cheats sets Hard in World Options and applies it
- **THEN** the server's difficulty becomes Hard and both clients show Hard

#### Scenario: Lock difficulty
- **WHEN** the owner locks the difficulty and confirms
- **THEN** the difficulty controls turn inactive, further change requests are refused, and the next session of that world starts locked

#### Scenario: Hardcore
- **WHEN** the world is hardcore
- **THEN** the difficulty button and lock stay inactive and a change request is refused

### Requirement: Game rules in Java's World Options
In a local mcr world, Java's Edit Game Rules SHALL behave as in vanilla singleplayer: active when the world allows cheats (the owner is an operator), greyed out otherwise, showing the server's current values and applying changed rules on the managed server. The server SHALL stay the one that checks permission.

#### Scenario: Cheats on
- **WHEN** the owner of a local world with cheats opens Edit Game Rules, turns `keep_inventory` on and clicks Done
- **THEN** the screen showed the server's values and the server's `keep_inventory` is true

#### Scenario: Cheats off
- **WHEN** the world does not allow cheats
- **THEN** Edit Game Rules is greyed out, as in vanilla
