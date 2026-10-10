## ADDED Requirements

### Requirement: Default render distance
The default render distance SHALL be 32 chunks and SHALL be sent in client information; the server MAY cap it.

#### Scenario: Fresh options
- **WHEN** no `options.txt` exists
- **THEN** render distance is 32 and simulation distance keeps its vanilla default
