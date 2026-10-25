## ADDED Requirements

### Requirement: Vanilla section meshes
Section meshes SHALL emit the same quads per render layer as vanilla's section compiler: positions, UVs, colours, light and the render layer of each face.

#### Scenario: Section mesh harness
- **WHEN** `cargo xtask parity section_mesh` runs
- **THEN** every case matches vanilla

### Requirement: Section geometry is never dropped for space
Uploading a section SHALL grow a full cube, model or tint arena instead of dropping the section, up to the
adapter's storage-binding limit.

#### Scenario: Overfilled arena
- **WHEN** a renderer whose arenas hold only a few quads uploads a whole world
- **THEN** every section is drawn and the frame matches a renderer with roomy arenas
