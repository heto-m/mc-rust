## ADDED Requirements

### Requirement: Curse overlay in view
The elder guardian curse SHALL draw the elder guardian model in front of the camera as vanilla's
`ElderGuardianParticle` does.

#### Scenario: Ocean monument
- **WHEN** an elder guardian applies mining fatigue
- **THEN** its face appears over the view and fades out

### Requirement: Hand-held rockets, elytra capes and boat water
An attached firework rocket SHALL sit at its holder's hand, elytra SHALL use the skin's elytra or cape
texture, and boats SHALL mask the water inside their hull.

#### Scenario: Boat on water
- **WHEN** a boat floats on water
- **THEN** no water surface shows inside the hull

### Requirement: Particles near the camera and map label order
Smoke particles at the camera SHALL stay soft translucent sprites, and map decoration names SHALL be drawn
with their icon in decoration order.

#### Scenario: Campfire smoke
- **WHEN** the camera stands in campfire smoke
- **THEN** no dark square covers the view

### Requirement: Spear hit feedback
The first-person spear SHALL dip by vanilla's hit feedback curve after a kinetic hit.

#### Scenario: Charge hit
- **WHEN** a spear charge hits a mob
- **THEN** the held spear dips and returns
