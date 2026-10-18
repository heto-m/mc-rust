## ADDED Requirements

### Requirement: Effect vision
Night vision and conduit power SHALL brighten the lightmap, and Blindness and Darkness SHALL pull fog in
to 5 and 15 blocks with the darkness pulse.

#### Scenario: Night vision
- **WHEN** the player has Night Vision in an unlit cave
- **THEN** blocks render bright

### Requirement: Fluid and biome fog
Lava, powder snow and water SHALL use vanilla's fog environments, water colour and distance SHALL come from
the biome, and sky and fog colours SHALL come from biome attributes blended around the camera, with rain
fog, End flashes and the flat-world horizon.

#### Scenario: Swamp water
- **WHEN** the camera is underwater in a swamp
- **THEN** the fog colour is the swamp water fog colour
