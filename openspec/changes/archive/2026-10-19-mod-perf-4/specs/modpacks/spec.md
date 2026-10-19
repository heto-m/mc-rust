## ADDED Requirements

### Requirement: Indexed pack zips
In mod mode, listing a directory of a zip resource pack SHALL read only the entries under that directory, from an
index of the zip's entries sorted by name built once per open zip, and SHALL hand them out in the zip's own order.

#### Scenario: Big texture pack
- **WHEN** Java's resource reload lists hundreds of directories of a pack zip with tens of thousands of entries
- **THEN** each listing reads only its own entries, and the loaded sprites and models are the same as without
  the index
