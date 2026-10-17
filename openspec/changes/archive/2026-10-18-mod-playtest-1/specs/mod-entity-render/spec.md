## ADDED Requirements

### Requirement: Bounded per-frame section walks
Java SHALL NOT look up every visible section's dirty state on a frame where no section turned dirty and the visible list is unchanged, and its block-entity extraction SHALL walk only the visible sections whose mesh holds block entities. Meshless sections SHALL compile without waiting for neighbour chunks.

#### Scenario: Block entities still render
- **WHEN** the server places a chest and a sign in view
- **THEN** Java extracts both block entities

#### Scenario: Still camera
- **WHEN** the camera does not move and no block changes
- **THEN** Java's extraction does not walk the visible sections for dirty state
