# Spec Delta

## MODIFIED Requirements

### Requirement: Instanced rendering
The system SHALL draw all instances of one model and texture array with a single instanced draw per pipeline using per-instance bone matrices from a storage buffer, entity textures from dynamically growing texture arrays, directional lighting, tint and hurt overlay, and opaque, translucent and additive pipelines.

#### Scenario: Row of mobs
- **WHEN** a pig, zombie, creeper, cow, villager and armor stand are rendered offscreen
- **THEN** each appears textured with vanilla proportions
