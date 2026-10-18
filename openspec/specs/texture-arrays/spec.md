# texture-arrays Specification

## Purpose
Decode sprites and pack them into mip-mapped texture array layers grouped by size.

## Requirements

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

### Requirement: Cutout mip coverage
Mip levels of textures with fully transparent pixels (or whose `.mcmeta` `texture.mipmap_strategy` is `cutout`, `strict_cutout` or `dark_cutout`) SHALL keep the base level's alpha-test coverage like vanilla's `MipmapGenerator.scaleAlphaToCoverage`: the alpha scale is searched over five bisection steps against a 4x4 bilinear coverage estimate at cutoff 0.5 (0.3 for strict), then `alpha_cutoff_bias + 0.025` is added. `mean` textures keep a plain alpha average.

#### Scenario: Far kelp
- **WHEN** kelp is drawn at the third mip level
- **THEN** it keeps most of its base coverage instead of vanishing below the cutout

### Requirement: Animated cutout mips
The cutout coverage of an animated sprite SHALL be measured and rescaled over its whole frame strip, as vanilla mips the strip, so every frame of a mip level gets the same alpha scale.

#### Scenario: Animated kelp at a distance
- **WHEN** distant kelp plants animate through all their frames at the third and fourth mip levels
- **THEN** no frame loses all of its coverage, so the kelp does not blink

### Requirement: Dark cutout mips
Textures whose `.mcmeta` `texture.mipmap_strategy` is `dark_cutout` SHALL fill fully transparent texels with 3/4 of their darkest opaque colour and build each mip as `MipmapGenerator.darkenedAlphaBlend`: the linear-light channels of the non-empty texels, alpha included, summed and divided by four, followed by the cutout coverage rescale.

#### Scenario: Distant leaves
- **WHEN** leaves are drawn at a high mip level
- **THEN** their colour is darker than the plain average where the texture has holes
