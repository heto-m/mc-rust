## ADDED Requirements

### Requirement: Spectator page order
The spectator teleport pages SHALL order players and teams and pick their icons as vanilla does.

#### Scenario: Team page
- **WHEN** a spectator opens the team page
- **THEN** teams appear in vanilla's order with vanilla's icons

### Requirement: Autosave indicator
The HUD SHALL show the saving indicator while the local world saves, when the option is on.

#### Scenario: Autosave
- **WHEN** the managed server saves a local world
- **THEN** the indicator fades in and out

### Requirement: Linear overlay sampling
Camera overlay textures SHALL be sampled linearly as vanilla's texture overlay does.

#### Scenario: Pumpkin
- **WHEN** the player wears a carved pumpkin
- **THEN** the blur scales smoothly without hard texels
