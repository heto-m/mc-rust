## ADDED Requirements

### Requirement: Creaking heart idle
Among the sampled blocks, a creaking heart that is not uprooted, in an overworld dimension type between day ticks 12600 and 23400, with pale oak logs on all six sides, SHALL play `block.creaking_heart.idle` with probability 1/16.

#### Scenario: Night, enclosed
- **WHEN** such a heart is sampled
- **THEN** the idle sound plays in the blocks category

#### Scenario: Daytime or uprooted
- **WHEN** it is day or the heart is uprooted
- **THEN** nothing plays
