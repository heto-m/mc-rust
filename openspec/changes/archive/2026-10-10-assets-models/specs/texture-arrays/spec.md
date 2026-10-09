## ADDED Requirements

### Requirement: Sprite decoding and animation
The system SHALL decode PNG sprites to RGBA, including non-16x16 sizes, and parse `.mcmeta` animation (frames, frametime, interpolate, frame size).

#### Scenario: Animated strip
- **WHEN** a sprite has an animation mcmeta
- **THEN** its vertical strip is split into frames occupying consecutive layers, with the frame order and times exposed

### Requirement: Texture array grouping
The system SHALL assign each sprite one or more layers in texture array groups keyed by pixel size, and expose per sprite the group, first layer and transparency class.

#### Scenario: Different sizes
- **WHEN** sprites of 16x16 and 32x32 are added
- **THEN** they land in separate groups

### Requirement: Mip chains
The system SHALL generate mip levels CPU-side with an alpha-weighted box filter.

#### Scenario: Transparent texels
- **WHEN** a 2x2 block has one opaque and three fully transparent texels
- **THEN** the mip texel takes the opaque texel's colour with alpha averaged
