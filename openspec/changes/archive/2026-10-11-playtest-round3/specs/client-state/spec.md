## ADDED Requirements

### Requirement: Entity event state
The client SHALL apply the state changes of vanilla's client `handleEntityEvent`: event 55 swaps another entity's main and off hand equipment, a ravager's event 69 knocks the local player back within 4 blocks of its box, a fishing hook's event 31 pulls a hooked local player toward the angler, events 22 and 23 set reduced debug info, and event 9 ends the local player's item use.

#### Scenario: Ravager roar
- **WHEN** a ravager two blocks away roars
- **THEN** the local player's velocity gains vanilla's strong knockback away from it

#### Scenario: Out of range
- **WHEN** a ravager twenty blocks away roars
- **THEN** the local player's velocity does not change
