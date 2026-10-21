# ui-render Specification

## Purpose
Sprite atlas, batched wgpu 2D renderer, GUI scale and the debug, chat and HUD proof screens.

## Requirements

### Requirement: GUI sprite atlas
The system SHALL pack `textures/gui/sprites/**` into an atlas and honour `.mcmeta` `gui.scaling` of `stretch`, `tile` and `nine_slice` (including border).

#### Scenario: Nine-slice
- **WHEN** a nine-slice sprite is drawn larger than its source
- **THEN** corners keep their pixel size and edges and centre stretch

### Requirement: Batched immediate-mode renderer
The system SHALL provide a `UiFrame` recording `fill_rect`, `sprite`, `text`, `component` and scissor clips, and SHALL submit one draw call per texture page and scissor batch, using premultiplied alpha. The pipeline SHALL be created from a `wgpu::Device` and `TextureFormat`.

#### Scenario: Batching
- **WHEN** a frame draws many glyphs from one page and many fills
- **THEN** it issues a bounded number of draws independent of glyph count

### Requirement: GUI scale
The system SHALL compute the automatic GUI scale as the largest integer such that the framebuffer still fits 320x240 GUI pixels, and SHALL honour an explicit maximum.

#### Scenario: 1080p
- **WHEN** the framebuffer is 1920x1080
- **THEN** the scale is 4

### Requirement: Proof screens
The system SHALL draw an F3-style debug overlay from left and right line lists, a chat HUD with message fade, and a hotbar, health, hunger, armor and experience HUD from vanilla sprites given plain numbers.

#### Scenario: Offscreen rendering
- **WHEN** each screen is rendered offscreen
- **THEN** the result is a non-empty image saved as a PNG, skipped when no GPU or jar exists

### Requirement: Dynamic sprite reuse
Sprites added at runtime SHALL be freed when dropped, and a later sprite of the same size SHALL reuse the freed atlas slot, so atlas pages do not grow without bound. Runtime icon, favicon and head caches SHALL be bounded.

#### Scenario: Icon cache churn
- **WHEN** thousands of distinct dyed item icons are drawn over a session
- **THEN** the number of atlas pages stays bounded

### Requirement: HUD elements
The HUD SHALL draw status effect icons, the attack indicator per option, the selected item name, mount
hearts and the jump bar, hunger icons, zero-saturation jitter, vanilla air bubbles, the off-hand slot on the
main-hand side, tab list hearts, the jukebox now-playing message, subtitles and the sleep fade, as `Hud`
does.

#### Scenario: Speed potion
- **WHEN** the player has Speed
- **THEN** its icon shows in the beneficial row at the top right

### Requirement: Paletted atlas textures
Item textures that exist only as the output of an atlas `paletted_permutations` source, such as the
armour trim overlays and their darker variants, SHALL be generated from the base texture and the key and
permutation palettes, as vanilla's atlas loader does.

#### Scenario: Gold-trimmed netherite helmet
- **WHEN** a netherite helmet carries a gold trim
- **THEN** its icon shows the gold trim overlay, not the missing texture

### Requirement: Crosshair and overlays
The crosshair SHALL use an inverting blend and draw only in first person (and in spectator only over a
menu provider). Camera overlays (pumpkin, powder snow, portal, nausea, fire, in-block, underwater,
spyglass, vignette) SHALL draw beneath the HUD.

#### Scenario: Scoping
- **WHEN** the player scopes with a spyglass
- **THEN** the hotbar and chat stay visible over the scope mask

### Requirement: Spectator menu
In spectator mode the item hotbar SHALL be replaced by the spectator menu, whose number keys teleport to
players and teams.

#### Scenario: Teleport to player
- **WHEN** a spectator opens the menu and picks a player
- **THEN** a teleport-to-entity packet with that player's UUID is sent

### Requirement: Spectator page order
The spectator teleport pages SHALL order players and teams and pick their icons as vanilla does.

#### Scenario: Team page
- **WHEN** a spectator opens the team page
- **THEN** teams appear in vanilla's order with vanilla's icons

### Requirement: Autosave indicator
The HUD SHALL show the saving indicator while the local world saves, when the option is on.

#### Scenario: Autosave
- **WHEN** the managed server saves a local world
- **THEN** the indicator fades in and out

### Requirement: Linear overlay sampling
Camera overlay textures SHALL be sampled linearly as vanilla's texture overlay does.

#### Scenario: Pumpkin
- **WHEN** the player wears a carved pumpkin
- **THEN** the blur scales smoothly without hard texels
