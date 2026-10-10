# app-screens Specification

## Purpose
App flow and in-game screens: title, server list, connecting, disconnected, inventory and containers, pause, death, chat and options, with actions routed to the client.

## Requirements

### Requirement: App flow
The app SHALL start on the title screen, reach the server list from Multiplayer, connect through a connecting screen, and show the disconnected screen with the reason when a session ends, returning to the server list.

#### Scenario: Disconnect
- **WHEN** the server closes the connection during play
- **THEN** the disconnected screen shows the reason and its button returns to the server list

#### Scenario: Direct connect
- **WHEN** `--connect host` is given
- **THEN** the app joins without showing menus

### Requirement: In-game screens
The app SHALL open the player inventory on the inventory key, the matching container screen on `open_screen`, the pause screen on Escape and the death screen on death, release the mouse while any screen is open, and translate screen actions into client calls.

#### Scenario: Container click
- **WHEN** a slot is clicked in a container screen
- **THEN** a container click carrying the current state id and predicted changed slots is sent

#### Scenario: Respawn
- **WHEN** Respawn is pressed on the death screen
- **THEN** a respawn client command is sent

### Requirement: Server click events need confirmation
The app SHALL open URLs, run commands and copy text from server-originated components only after the confirm screens in the UI crate return the action.

#### Scenario: URL
- **WHEN** `OpenUrl` reaches the app
- **THEN** only http and https URLs are opened

### Requirement: Options screens
The app SHALL provide Video, Sound, Controls and Chat option screens that write `options.txt` and apply changes without restart, sending client information when render distance changes.

#### Scenario: Render distance
- **WHEN** render distance is changed in Video options
- **THEN** the server is sent updated client information and the local unload radius changes
