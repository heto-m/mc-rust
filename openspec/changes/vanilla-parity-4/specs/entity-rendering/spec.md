## ADDED Requirements

### Requirement: Invisible entities
An invisible entity's body SHALL not be drawn unless the viewer may see invisible entities (spectator or
a team that sees friendly invisibles), in which case it SHALL be translucent. Armour and held items SHALL
still draw.

#### Scenario: Invisibility potion
- **WHEN** another player drinks Invisibility while wearing no armour
- **THEN** nothing of that player is drawn except held items

### Requirement: Use and pose states
First-person and third-person rendering SHALL animate item use for the hand that uses it, show the bow,
shield and crossbow use states on other entities, seat the local player when riding, and show burning
entities on fire.

#### Scenario: Off-hand shield
- **WHEN** the player blocks with an off-hand shield
- **THEN** the off-hand shield is raised in first person

### Requirement: Camera effects
The world view SHALL bob with view bobbing, tilt on damage, warp under nausea and in portals, and apply the
bow, death and fluid FOV changes with the vanilla clamp.

#### Scenario: Drawing a bow
- **WHEN** the player draws a bow fully
- **THEN** the FOV narrows by 15%

### Requirement: Particle providers
Every vanilla particle type SHALL have its own behaviour instead of the generic drift, including falling
leaves, tinted falling dust and dust pillars.

#### Scenario: Cherry leaves
- **WHEN** a cherry leaves block spawns a particle
- **THEN** it falls with gravity and lives up to 300 ticks
