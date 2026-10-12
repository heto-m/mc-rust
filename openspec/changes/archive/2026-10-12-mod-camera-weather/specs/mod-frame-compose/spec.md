## MODIFIED Requirements

### Requirement: Virtual main target
Textures created for Java's main render target SHALL be marked virtual. In the GUI slot, a pass into Java's main color SHALL draw into mcr's `Rgba8Unorm` scene after mcr's world passes. Projected geometry SHALL land top-down, and scissor rectangles SHALL be flipped to match, so no copy of the scene is made. Java's main depth in the GUI slot SHALL be a private depth. Java clears of the virtual color target SHALL be ignored. A copy that Java reads from its main color SHALL see the current scene in Java's row order.

#### Scenario: HUD over world
- **WHEN** Java draws its HUD while joined to a server
- **THEN** mcr's world remains visible behind the hotbar, hearts and crosshair

#### Scenario: Menu blur
- **WHEN** Java opens a screen that blurs the background
- **THEN** the blur samples mcr's rendered world

#### Scenario: No flip copies
- **WHEN** a mod-mode frame composes Java's GUI in game
- **THEN** the compositor records no fullscreen copy of the scene
