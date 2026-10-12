## MODIFIED Requirements

### Requirement: World ownership split
In mod mode, Java SHALL render entities, block entities, nametags, the hand, outlines, post effects, mod particle types and vanilla particles spawned by mod code outside the paths mcr mirrors. mcr SHALL render terrain, sky, clouds, weather, world border, block outline, destroy overlay and vanilla particles from packets and vanilla ticks, and SHALL NOT render entities, block entities or nametags natively.

#### Scenario: No double drawing
- **WHEN** a zombie stands in view in mod mode
- **THEN** it is drawn once, by Java's entity renderer

#### Scenario: Server particle
- **WHEN** the server sends a `minecraft:flame` particle packet
- **THEN** mcr draws it and Java does not create it

#### Scenario: Mod spawns a vanilla particle
- **WHEN** a mod's client tick handler spawns `minecraft:happy_villager`
- **THEN** Java creates and draws it

## ADDED Requirements

### Requirement: Shared render distance
Java's effective render distance and entity distance scaling SHALL follow mcr's every frame, so fog on Java-rendered entities matches mcr's terrain fog and Java culls entities at mcr's distance.

#### Scenario: Render distance changed in mcr
- **WHEN** the user sets mcr's render distance to 6
- **THEN** Java's effective render distance is 6 from the next frame on
