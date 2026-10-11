## ADDED Requirements

### Requirement: World ownership split
In mod mode, Java SHALL render entities, block entities, nametags, the first-person hand, glowing outlines, screen post effects and particles of non-`minecraft` types. mcr SHALL render terrain, sky, clouds, weather, world border, block outline, destroy overlay and vanilla particles, and SHALL NOT render entities, block entities or nametags natively.

#### Scenario: No double drawing
- **WHEN** a zombie stands in view in mod mode
- **THEN** it is drawn once, by Java's entity renderer

### Requirement: Meshless sections
Java's section compilation SHALL produce no geometry and SHALL list each section's renderable block entities, so that vanilla's section visibility and block entity extraction work without terrain meshing.

#### Scenario: Chest renders
- **WHEN** the server places a chest in view
- **THEN** Java renders the chest within one second and Java builds no terrain vertex data

### Requirement: Shared camera and depth
Java's camera and level projection SHALL equal mcr's for each frame, and Java world-space draws SHALL depth-test against mcr's reverse-Z scene depth.

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
