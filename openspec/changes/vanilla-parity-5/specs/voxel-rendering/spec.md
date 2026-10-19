## ADDED Requirements

### Requirement: Improved transparency
With `improvedTransparency` on, translucent terrain, particles, weather, clouds and item entities SHALL be
composited in layers as vanilla's Fabulous mode does.

#### Scenario: Water behind glass
- **WHEN** water is seen through stained glass with improved transparency on
- **THEN** both blend in depth order

### Requirement: Filtering and leaf shading cost
Texture filtering NONE SHALL sample as vanilla's linear `sampleNearest` path does, and cutout terrain SHALL
not be shaded behind solid terrain more often than vanilla's SOLID-then-CUTOUT order allows, without
changing the picture.

#### Scenario: Forest
- **WHEN** a forest is drawn at render distance 16
- **THEN** the frame is pixel-identical and GPU time is no higher than with one shared pass
