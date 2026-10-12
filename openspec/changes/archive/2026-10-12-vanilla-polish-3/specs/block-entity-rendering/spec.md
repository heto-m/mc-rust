## ADDED Requirements

### Requirement: Block entity texture reuse
The skull skin cache SHALL free the layer of a replaced skin, drop skins unused for 1800 frames and hold at most 128 skins.

#### Scenario: Replaced skin
- **WHEN** the skin for a URL is set twice
- **THEN** the first layer is freed
