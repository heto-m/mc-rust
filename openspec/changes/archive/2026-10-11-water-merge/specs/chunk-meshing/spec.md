## ADDED Requirements

### Requirement: Merged still fluid surfaces
Flat, still fluid top faces SHALL be merged into rectangles of at most 3x3 blocks when they share Y, corner height, sprite, light, back-face flag and resolved biome tint colour. The merged quad SHALL repeat the sprite once per block and cover exactly the area of the separate faces. Flowing, sloped, side and bottom faces SHALL NOT be merged.

#### Scenario: Flat ocean
- **WHEN** a 16x16 section is filled with still water below air
- **THEN** its top surface is emitted as 3x3-block quads instead of one quad per block, covering the same area

#### Scenario: Differing light or tint
- **WHEN** adjacent top faces differ in light or in the resolved tint colour of their columns
- **THEN** they are not merged

#### Scenario: Flowing water
- **WHEN** a top face has a horizontal flow vector
- **THEN** it keeps its own rotated quad
