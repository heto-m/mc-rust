## ADDED Requirements

### Requirement: Loading screens keep the last frame
Java SHALL flag frames drawn under loading, connecting, message and progress screens and mcr's world screen. While Java's frames are flagged, a late Java frame SHALL keep the last composed image as while Java loads, and SHALL not detach the bridge before the loading limit.

#### Scenario: JEI starts on join
- **WHEN** a modded join spends 2.3 seconds in one Java frame under the level loading screen
- **THEN** mcr keeps showing the last composed frame and does not report that mods stopped drawing
