## ADDED Requirements

### Requirement: Precipitation columns
The renderer SHALL draw rain and snow columns around the camera within the weather radius using a source that supplies the motion-blocking height, precipitation kind and light.

#### Scenario: Snow biome
- **WHEN** the source reports snow for a column
- **THEN** the snow texture is used with slower, swaying movement

#### Scenario: Radius bound
- **WHEN** a radius above ten is requested
- **THEN** it is clamped to ten

### Requirement: Lightning flash
The renderer SHALL expose a flash level that brightens the sky colour and sky light factor.

#### Scenario: Flash
- **WHEN** a flash is triggered
- **THEN** the sky colour is lerped towards (0.8, 0.8, 1.0) by 0.22 until it expires
