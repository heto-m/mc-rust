## ADDED Requirements

### Requirement: Singleplayer under NeoForge
Java's world screens SHALL drive mcr's managed vanilla server when NeoForge is the loader exactly as under Fabric: create, play, pause with tick freeze, Save and Quit, reopening a world and Open to LAN.

#### Scenario: Create, save and reopen under NeoForge
- **WHEN** the user creates a world on Java's Create World screen under NeoForge, plays, pauses, saves and quits, and plays it again from Java's list
- **THEN** mcr's server generates the world, the pause freezes ticks, Save and Quit returns to Java's title screen, and the second join loads the same world

#### Scenario: Open to LAN under NeoForge
- **WHEN** the user opens the world to LAN from World Options under NeoForge
- **THEN** mcr's forwarder starts and Java shows vanilla's chat line with the port
