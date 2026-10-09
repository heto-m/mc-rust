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
