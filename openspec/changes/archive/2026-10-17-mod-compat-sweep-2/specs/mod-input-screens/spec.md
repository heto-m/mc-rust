## ADDED Requirements

### Requirement: Movement in Java screens
While a Java screen is open, Java SHALL report which movement keys a mod let through to the player's input, and mcr SHALL move its player with exactly those keys. Without such a mod the player SHALL not move while a Java screen is open.

#### Scenario: InvMove-style mod
- **WHEN** a mod keeps the movement keys active in the inventory and the user holds forward with the inventory open
- **THEN** mcr's player walks forward

#### Scenario: No such mod
- **WHEN** the user holds forward with the inventory open and no mod lets movement through
- **THEN** mcr's player stands still

### Requirement: Raw SDL event hooks
Key and mouse button events SHALL also pass through vanilla's SDL event handler methods as SDL events, with vanilla's own handling there skipped, so that mods hooking those methods see them. An event a mod cancels there SHALL not reach Java's keyboard or mouse handler.

#### Scenario: Hotkey library
- **WHEN** a mod hooks vanilla's SDL key event handler and cancels E
- **THEN** pressing E in game does not open the inventory

#### Scenario: No hook
- **WHEN** no mod hooks the SDL event handler
- **THEN** keys and buttons reach Java exactly once, as before
