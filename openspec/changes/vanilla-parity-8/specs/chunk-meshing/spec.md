## ADDED Requirements

### Requirement: Vanilla section meshes
Section meshes SHALL emit the same quads per render layer as vanilla's section compiler: positions, UVs, colours, light and the render layer of each face.

#### Scenario: Section mesh harness
- **WHEN** `cargo xtask parity section_mesh` runs
- **THEN** every case matches vanilla
