## ADDED Requirements

### Requirement: Particle fog
Particles SHALL fade into the environment and render-distance fog by the same distances as terrain.

#### Scenario: Far particles
- **WHEN** particles are seen near the end of the render distance
- **THEN** they take the fog colour as terrain at that distance does
