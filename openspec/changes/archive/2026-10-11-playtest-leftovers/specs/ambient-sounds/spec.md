## MODIFIED Requirements

### Requirement: Underwater ambience
The client SHALL play enter and exit sounds when the local player's physics eye-in-water state changes (vanilla `isEyeInFluid(WATER)`, where a water block with water above is full height), a looping underwater ambience that fades in over 40 ticks and out twice as fast, and random additions with per-tick chances of 1%, 0.1% and 0.01% while underwater.

#### Scenario: Diving
- **WHEN** the eye enters water
- **THEN** the enter sound plays and the underwater loop begins

#### Scenario: Sinking to a pool bottom
- **WHEN** the player sinks through a deep pool and rests on its floor with the eye below the surface
- **THEN** the enter sound plays once and no exit sound plays, including while the eye passes the top ninth of fully submerged blocks
