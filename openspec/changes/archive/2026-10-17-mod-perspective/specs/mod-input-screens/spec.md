## ADDED Requirements

### Requirement: Perspective key in mod mode
While Java leads the perspective, the toggle-perspective key SHALL reach Java's vanilla key mapping and mcr SHALL NOT cycle its own perspective on it. Rebinding the key in a Java screen SHALL rebind mcr's mapping as for every vanilla key.

#### Scenario: F5 with mods
- **WHEN** Java draws the world and the user presses F5
- **THEN** Java's toggle-perspective mapping is clicked once, and mcr's perspective changes only by following Java

#### Scenario: Rebound in Controlling
- **WHEN** the user binds toggle perspective to P in a Java screen and presses P in game
- **THEN** the perspective changes
