## ADDED Requirements

### Requirement: sounds.json model
The system SHALL parse `sounds.json` into events whose entries are either sound files or `type: event` references, with `weight`, `volume`, `pitch`, `stream`, `attenuation_distance` and `preload` fields defaulting to 1, 1, 1, false, 16 and false.

#### Scenario: Defaults
- **WHEN** an entry is the bare string `"block/stone/step1"`
- **THEN** it has weight 1, volume 1, pitch 1, attenuation distance 16 and is not streamed

#### Scenario: Event reference
- **WHEN** an entry has `"type": "event"`
- **THEN** selection resolves it by picking from the referenced event, scaling volume and pitch by the reference's values

### Requirement: Seeded weighted selection
`SoundManager::play` SHALL pick a variant by weight using an RNG seeded with the caller's seed, so equal seeds give equal choices.

#### Scenario: Determinism
- **WHEN** the same event is picked twice with the same seed
- **THEN** the same variant is chosen

#### Scenario: Weights
- **WHEN** a variant has weight 3 and another weight 1, over many seeds
- **THEN** the first is chosen about three times as often

#### Scenario: Unknown event
- **WHEN** the event id is not defined
- **THEN** nothing is played and no error is raised
