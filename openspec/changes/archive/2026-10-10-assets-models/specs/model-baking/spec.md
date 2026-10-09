## ADDED Requirements

### Requirement: Blockstate selection
The system SHALL select models for a block name and property list using `variants` (property-match keys, weighted lists) or `multipart` (`when` with single conditions, `OR`, `AND`, `|` alternatives and `!` negation).

#### Scenario: Variant key match
- **WHEN** a blockstate defines a variant key that is a subset of the given properties
- **THEN** that variant's model is baked

#### Scenario: Multipart conditions
- **WHEN** several multipart selectors match
- **THEN** each matching selector contributes one part, in definition order

#### Scenario: Weighted variants
- **WHEN** a variant is a weighted list
- **THEN** every alternative is baked and carries its weight

### Requirement: Model inheritance and textures
The system SHALL resolve block models through parent chains, inheriting elements, ambient occlusion and texture variables, and resolve `#variable` chains to sprites.

#### Scenario: Child overrides parent texture
- **WHEN** a child redefines a texture variable
- **THEN** inherited elements use the child's texture

#### Scenario: Unresolved variable
- **WHEN** a face references a variable with no sprite
- **THEN** baking reports an error naming the model and variable

### Requirement: Quad baking
The system SHALL bake elements into quads with block-space positions, sprite-local UVs, normal direction, cull direction, tint index, shading direction and light emission, supporting element rotation (single axis or Euler, with rescale).

#### Scenario: Default UVs
- **WHEN** a face has no `uv`
- **THEN** UVs derive from the element bounds and face direction

### Requirement: Blockstate rotation and uvlock
The system SHALL apply the variant's x/y/z quarter-turn rotation about the block centre, rotate cull directions accordingly, and with `uvlock` keep textures aligned to the world.

#### Scenario: Y rotation of cull face
- **WHEN** a variant has `y: 90` and a face culls `north`
- **THEN** the baked quad culls `east`

#### Scenario: uvlock
- **WHEN** a variant rotates a face with `uvlock`
- **THEN** that face's UVs are counter-rotated so the texture does not turn

### Requirement: Full-jar bakeability
Every blockstate in the vanilla client jar SHALL bake without error.

#### Scenario: Real jar
- **WHEN** all blockstate files of the 26.3 client jar are baked
- **THEN** the failure count is zero
