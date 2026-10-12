# ambient-sounds Specification

## MODIFIED Requirements

### Requirement: Block and fluid ambience
The block and fluid ambient sounds SHALL come from the shared `animateTick` scan of the `block-animate-ticks` capability: lit campfires, furnaces, blast furnaces, smokers and candles, fire, nether portals, charged respawn anchors, bubble columns, open eyeblossoms, dried ghasts, firefly bushes, potent sulfur, pale hanging moss, poplar leaves, desert sand, dead bushes and dry grass, flowing water and lava with air above, each with the vanilla probability.

#### Scenario: Lit furnace
- **WHEN** a lit furnace is sampled
- **THEN** its crackle plays with probability 0.1

#### Scenario: Unlit furnace
- **WHEN** an unlit furnace is sampled
- **THEN** nothing plays

#### Scenario: Java owns audio
- **WHEN** the mod bridge owns world sounds
- **THEN** the scan spawns particles only
