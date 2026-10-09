# Spec Delta

## ADDED Requirements

### Requirement: Render layers
The renderer SHALL draw opaque and cutout geometry in the main pass and translucent geometry in a later
pass ordered back to front by section distance, with alpha blending and no depth writes.

#### Scenario: Translucent over opaque
- **WHEN** a translucent section lies in front of an opaque one
- **THEN** the opaque colour shows through the translucent surface

### Requirement: Texture groups
The block texture array SHALL be uploadable as several size groups with mip chains, addressed by a group
and layer packed into the quad layer field.

#### Scenario: Mixed sizes
- **WHEN** sprites of two sizes are uploaded
- **THEN** quads of each size sample the right texels

### Requirement: Tint tables
Biome tints SHALL be looked up per block column from a per-section table selected by the quad tint kind,
with constant tints from a global palette.

#### Scenario: Untinted quad
- **WHEN** a quad has tint kind zero
- **THEN** its colour is the texture colour times lighting only
