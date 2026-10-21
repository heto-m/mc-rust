## ADDED Requirements

### Requirement: Local suggestion providers
The client SHALL fill the `available_sounds`, `summonable_entities` and `post_effects` suggestion providers
locally.

#### Scenario: Summon
- **WHEN** the player types `/summon minecraft:zo`
- **THEN** the suggestions list zombie entity types without asking the server
