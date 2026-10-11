## ADDED Requirements

### Requirement: Firework sparks
The engine SHALL simulate firework sparks with vanilla's gravity, friction and lifetime, a spark colour that fades to an optional fade colour in the second half of life, trail copies spawned half-aged, and twinkling that hides the spark in alternating three-tick windows after the first third of its life. The firework flash SHALL grow and fade over four ticks.

#### Scenario: Trail
- **WHEN** a trailing spark ticks during the first half of its life
- **THEN** it leaves stationary half-aged copies of itself

#### Scenario: Flash
- **WHEN** a firework flash is added
- **THEN** it is drawn for four ticks with a size following vanilla's sine curve and then removed
