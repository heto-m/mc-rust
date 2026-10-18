## ADDED Requirements

### Requirement: Game rules in Java's World Options
In a local mcr world, Java's Edit Game Rules SHALL behave as in vanilla singleplayer: active when the world allows cheats (the owner is an operator), greyed out otherwise, showing the server's current values and applying changed rules on the managed server. The server SHALL stay the one that checks permission.

#### Scenario: Cheats on
- **WHEN** the owner of a local world with cheats opens Edit Game Rules, turns `keep_inventory` on and clicks Done
- **THEN** the screen showed the server's values and the server's `keep_inventory` is true

#### Scenario: Cheats off
- **WHEN** the world does not allow cheats
- **THEN** Edit Game Rules is greyed out, as in vanilla
