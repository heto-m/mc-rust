# mod-entity-render Specification

## Purpose
In mod mode, Java draws entities, block entities, nametags, the hand, outlines and post effects into mcr's world, sharing mcr's camera and depth, while mcr keeps terrain, sky and world effects; Java's web traffic leaves only through a Rust loopback proxy.

## Requirements

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

### Requirement: Meshless sections
Java's section compilation SHALL produce no geometry and SHALL list each section's renderable block entities, so that vanilla's section visibility and block entity extraction work without terrain meshing.

#### Scenario: Chest renders
- **WHEN** the server places a chest in view
- **THEN** Java renders the chest within one second and Java builds no terrain vertex data

### Requirement: Shared camera and depth
Java's camera and level projection SHALL equal mcr's render camera for each frame, and Java world-space draws SHALL depth-test against mcr's reverse-Z scene depth. mcr's render camera SHALL be its own player camera unless a Java camera handoff is active.

#### Scenario: Occlusion by terrain
- **WHEN** a zombie stands behind a stone wall relative to the camera
- **THEN** no zombie pixels are visible

### Requirement: Vanilla draw order
Java world draws SHALL interleave with mcr's passes in vanilla's order: solid features after opaque terrain, translucent features before translucent terrain, translucent-after-terrain features, outlines and always-on-top draws after mcr's world effects, then hand and post effects, then GUI.

#### Scenario: Entity behind water
- **WHEN** a zombie stands behind a water surface
- **THEN** the zombie is visible tinted through the water, as in vanilla

### Requirement: Hand depth isolation
The first-person hand and screen post effects SHALL render with a private depth buffer, so their depth clears never modify mcr's scene depth.

#### Scenario: Hand clear
- **WHEN** Java renders the first-person hand after clearing depth
- **THEN** mcr's scene depth after the frame equals its value before the hand pass

### Requirement: Mod entity renderers apply
Entity renderer and layer changes registered by mods through Fabric API or mixins SHALL be visible in mod mode.

#### Scenario: Test layer
- **WHEN** the test mod registers an armor-stand feature layer and an armor stand is in view
- **THEN** the layer's pixels appear on the armor stand

### Requirement: Loopback web proxy
Java's HTTP and HTTPS requests SHALL go through a loopback proxy owned by Rust that reaches only allow-listed hosts (Mojang's plus the mod policy's `http_hosts`) on global addresses, with a bounded number of connections, request head size and idle time.

#### Scenario: Unlisted host
- **WHEN** Java asks the proxy for a host that is not on the allow-list
- **THEN** the proxy answers 403 and opens no upstream connection

### Requirement: Shared render distance
Java's effective render distance and entity distance scaling SHALL follow mcr's every frame, so fog on Java-rendered entities matches mcr's terrain fog and Java culls entities at mcr's distance.

#### Scenario: Render distance changed in mcr
- **WHEN** the user sets mcr's render distance to 6
- **THEN** Java's effective render distance is 6 from the next frame on

### Requirement: Camera handoff
When a Java mod makes Java's final camera differ from mcr's player camera, by moving or rotating it or by making another entity the camera entity, Java SHALL report that camera. While Java draws the world, mcr SHALL render the next frame's terrain from the reported camera, and Java SHALL render that frame from the same camera. When no mod moves the camera, frames SHALL be identical to frames without this feature.

#### Scenario: Camera moved by a mixin
- **WHEN** a mod moves Java's camera three blocks up at the end of its camera update
- **THEN** mcr renders terrain from three blocks above the player's eye, and the player does not move

#### Scenario: Mod releases the camera
- **WHEN** the mod makes the player the camera entity again
- **THEN** mcr renders from its own player camera

### Requirement: Handoff camera bounds
mcr SHALL ignore a reported camera with non-finite values, and SHALL clamp its position to the loaded world plus a margin.

#### Scenario: Hostile camera
- **WHEN** Java reports a camera with a NaN coordinate or one far outside the loaded chunks
- **THEN** mcr ignores the NaN report, or clamps the position to the loaded area plus the margin

### Requirement: Handoff input capture
Java SHALL report whether the mod took the player's movement input. While it has, mcr's player SHALL neither move nor turn, and Java SHALL receive the movement keys and mouse look.

#### Scenario: Free camera
- **WHEN** a mod makes another entity the camera entity and takes the player's movement input, and the user holds forward
- **THEN** mcr's camera moves forward with that entity, and mcr's player stays where it was

#### Scenario: Input returned
- **WHEN** the mod gives the player its movement input back
- **THEN** movement keys move mcr's player again

### Requirement: Weather replacement
When a mod cancels or redirects vanilla's weather extraction in Java, Java SHALL report it for the frame, and mcr SHALL then not draw its own rain and snow while Java draws the world.

#### Scenario: Weather mod
- **WHEN** a mod cancels `WeatherEffectRenderer.extractRenderState` while it rains
- **THEN** mcr draws no rain or snow columns, and mcr still draws its sky and fog for the rain

#### Scenario: No weather mod
- **WHEN** no mod touches weather rendering and it rains
- **THEN** mcr draws its rain as before

### Requirement: Section visibility for Java culling
mcr SHALL share the set of sections its visibility graph reached for the current render camera with Java. Java SHALL skip the renderable block entities of sections outside that set, and SHALL skip entities whose bounding boxes touch only sections outside it. Sections outside the shared area, and frames without a set, SHALL be culled as before.

#### Scenario: Mobs sealed in stone
- **WHEN** 500 zombies stand in an air pocket enclosed by solid stone sections, out of the camera's sight
- **THEN** Java extracts none of them, and its frame time drops accordingly

#### Scenario: Large entity across sections
- **WHEN** an entity's position lies in an unreached section but its bounding box reaches into a reached one
- **THEN** Java still draws it

### Requirement: Camera mods inside the alignment
Java's camera alignment SHALL run vanilla's own code with mcr's camera position and rotation substituted, rather than skipping it, so that mods hooking inside the alignment still change the camera. Their change SHALL reach mcr through the camera handoff.

#### Scenario: Rotation hook after the alignment's rotation
- **WHEN** a mod sets the camera's rotation right after vanilla's alignment set it, as Freelook does
- **THEN** Java reports the moved camera and mcr renders terrain from it

#### Scenario: No camera mod
- **WHEN** no mod hooks the camera alignment
- **THEN** Java's camera equals mcr's camera and nothing is reported

### Requirement: Mouse turn capture
Java SHALL report when a mod consumes the mouse turn of the local player before vanilla applies it. While it does and Java draws the world, mcr's player SHALL not turn with the mouse, and Java SHALL still receive the mouse motion.

#### Scenario: Freelook
- **WHEN** a mod cancels the local player's turn and turns only its camera, and the user moves the mouse
- **THEN** mcr's player keeps its rotation and the camera turns

#### Scenario: No turn mod
- **WHEN** no mod consumes the turn and the user moves the mouse
- **THEN** mcr's player turns as before

### Requirement: Camera roll handoff
When a mod rolls Java's camera about its view axis, Java SHALL report the roll with the handed-off camera, and mcr SHALL roll its view by it on top of its own tilts. Java SHALL draw its world with the same roll mcr renders, so entities stay aligned with mcr's terrain.

#### Scenario: Rolled camera
- **WHEN** a mod rolls the camera's rotation 15 degrees about its view axis
- **THEN** mcr's view rolls by the same angle and Java's entities stay on mcr's ground

#### Scenario: No roll
- **WHEN** no mod rolls the camera
- **THEN** the reported roll is 0 and nothing is handed off

### Requirement: Third-person pull-back from the eye
In third person, Java's camera alignment SHALL start from mcr's eye and pull back by mcr's camera distance, so that without a mod Java's camera equals mcr's, and a mod that replaces the pull-back offsets the camera from the eye as in vanilla.

#### Scenario: Shoulder camera
- **WHEN** a mod redirects the third-person pull-back to add a sideways offset
- **THEN** mcr renders from the offset camera

#### Scenario: Plain third person
- **WHEN** no mod changes the third-person camera
- **THEN** nothing is handed off and mcr renders its own third-person camera

### Requirement: Java leads the perspective
While Java draws the world, Java's camera type SHALL decide the perspective. Java SHALL report it each frame as first person, third person back or third person front, and mcr SHALL follow it for its world camera, its native player model and its hand. A mod perspective on top of a camera type SHALL reach mcr through that camera type and the camera handoff.

#### Scenario: Mod perspective cycle
- **WHEN** a mod consumes F5 and switches Java from first person straight to third person front
- **THEN** mcr's perspective becomes third person front and its camera looks back at the player

#### Scenario: Shoulder view
- **WHEN** a mod switches Java to third person back and offsets the pull-back sideways
- **THEN** mcr follows into third person and renders from the offset camera

#### Scenario: Without mods
- **WHEN** no mods are loaded and the user presses F5
- **THEN** mcr cycles first person, third person back and third person front itself, as before

### Requirement: Perspective switch frame
Java SHALL NOT force its camera type to mcr's perspective. On a frame where Java's camera type and mcr's perspective disagree, Java's alignment SHALL run vanilla's own code, and the resulting camera SHALL reach mcr through the camera handoff until mcr has followed.

#### Scenario: Switch to third person
- **WHEN** Java switches to third person back while mcr is still in first person
- **THEN** mcr renders from Java's pulled-back camera and then from its own third-person camera, never from the eye with the player model visible

### Requirement: Mod roll of the level pose
Java SHALL report the roll about the view axis that mods add to the level's bob pose, as the difference between the pose vanilla's bob steps built and the pose vanilla alone builds from the same state. mcr SHALL roll its view by it on top of its own tilts while Java draws the world.

#### Scenario: Camera Overhaul style roll
- **WHEN** a mod rotates the bob pose stack 10 degrees about the view axis at the start of `bobHurt`
- **THEN** mcr's view rolls by 10 degrees and Java's entities roll with mcr's terrain

#### Scenario: No pose mod
- **WHEN** no mod changes the bob pose, including while hurt or walking with view bobbing
- **THEN** the reported roll is 0
