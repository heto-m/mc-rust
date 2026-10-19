## ADDED Requirements

### Requirement: The C heap is trimmed after loads
On Linux with glibc, mcr SHALL return free C heap memory to the system (`malloc_trim`) off the render thread once
the title assets loaded, once Java's title screen appears, and when section meshing stopped for two seconds after
it ran, at most once every twenty seconds for the latter.

#### Scenario: After the join
- **WHEN** the join's chunks are meshed and meshing stops
- **THEN** glibc's free arena memory (about 600 MB in Medieval Immersed) is returned within a few seconds

### Requirement: Replaced models are released
Readers that only need resource files (the HUD's waypoint styles, default skins) SHALL hold the resource manager,
not the baked assets, so a replaced model cache is freed.

#### Scenario: Rebake after Java's models
- **WHEN** the model cache is rebaked in game
- **THEN** only one model cache stays alive
