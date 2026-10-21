## ADDED Requirements

### Requirement: Complete improved transparency
With `improvedTransparency` on, translucent entity layers, item entities, weather, clouds and the world
border SHALL be composited with the translucent world, the composite SHALL write depth, and dynamic lights
SHALL light translucent terrain.

#### Scenario: Entity behind water
- **WHEN** a translucent entity layer is seen through water with improved transparency on
- **THEN** both blend in depth order

### Requirement: Late-phase leaf shading cost
Sections disoccluded in the late cull phase SHALL draw solid quads before cutout quads without changing the
picture.

#### Scenario: Forest
- **WHEN** a forest is viewed with culling on
- **THEN** frames are pixel-identical to the single discarding pass
