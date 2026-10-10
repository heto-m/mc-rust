## ADDED Requirements

### Requirement: Destroy stage decals
The renderer SHALL overlay destroy stage 0 to 9 textures on blocks with reported breaking progress, following the block's model boxes.

#### Scenario: Progress
- **WHEN** a block has progress 5
- **THEN** stage texture 5 is drawn over its faces

#### Scenario: Cleared
- **WHEN** progress is reported as out of range
- **THEN** the decal is removed
