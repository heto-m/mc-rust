## ADDED Requirements

### Requirement: Shared quad runs
The model cache SHALL store identical compiled quad runs once and let every state and variant that uses them point at the same run.

#### Scenario: Vanilla cache
- **WHEN** the vanilla 26.3 block models are baked
- **THEN** the cache holds fewer than half the quads of one run per state

### Requirement: Sprite size classes
Block sprites SHALL fit the renderer's four texture groups: the four most used sizes, 16×16 always among them, are kept, and every other sprite SHALL be resampled to the nearest kept size, box-filtered where the size divides.

#### Scenario: Large animated fluid texture
- **WHEN** a mod's fluid texture is 256×256 per frame and four smaller sizes are in use
- **THEN** it is resampled into a kept size and renders with its texture, not as missing
