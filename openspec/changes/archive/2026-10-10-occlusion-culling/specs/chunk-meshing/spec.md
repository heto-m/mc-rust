## ADDED Requirements

### Requirement: Section connectivity
Meshing SHALL record, for every section including ones without geometry, which pairs of its six faces are
connected through cells that are not opaque full blocks, treating a section with fewer than 256 opaque
cells as fully connected.

#### Scenario: Solid floor
- **WHEN** a section is split by a full horizontal layer of opaque blocks
- **THEN** its top and bottom faces are not connected while each still connects to the sides

#### Scenario: Tunnel
- **WHEN** a section is solid except for a straight tunnel between its east and west faces
- **THEN** only the east and west faces are connected

### Requirement: Enclosed leaf face culling
With leaves culling enabled (the default), a face between two leaf blocks SHALL be omitted when both blocks
have only leaves or opaque full blocks as their six neighbours; faces of leaves touching any other block keep
the fancy-leaves behaviour.

#### Scenario: Leaf blob
- **WHEN** a 5x5x5 cube of leaves is meshed
- **THEN** only the faces between cells of its enclosed 3x3x3 core are missing
