## ADDED Requirements

### Requirement: Sky light of sections without data
Sky light SHALL be read like vanilla's `SkyLightSectionStorage`: a light section without data takes the bottom layer of the nearest section above it that has data, and 15 when no section above has any. Meshing, entity and particle lighting and the debug screen SHALL use this rule.

#### Scenario: Deep ocean floor
- **WHEN** the sections below a lit water section carry no sky light data
- **THEN** the sea floor there reads the lit section's bottom layer, 0 under 15 blocks of water
