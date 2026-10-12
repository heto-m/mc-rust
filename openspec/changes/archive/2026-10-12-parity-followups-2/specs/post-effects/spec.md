## ADDED Requirements

### Requirement: Resource-pack post chains
A resource pack's `assets/minecraft/post_effect/<chain>.json` for `blur`, `creeper`, `invert` or `spider` SHALL retune that chain only when it has the built-in chain's shape. Any other file, including one with pack shaders, SHALL leave the built-in chain unchanged and a warning SHALL be logged. `entity_outline` SHALL always stay built in.

#### Scenario: Retuned invert
- **WHEN** a pack's `invert.json` sets `InverseAmount` to 0.25 and keeps the rest
- **THEN** the invert chain inverts by 0.25

#### Scenario: Custom shader
- **WHEN** a pack's chain names `pack:post/custom` as a fragment shader
- **THEN** the built-in chain runs and a warning is logged

### Requirement: Pack chain shape
A pack chain SHALL have the same number of passes as the built-in chain, the same vanilla fragment and vertex shader ids, the same input and output targets and filters, only empty target definitions, and every uniform each pass reads.

#### Scenario: Different graph
- **WHEN** a pack chain writes a pass to another target or defines a sized target
- **THEN** it is ignored

### Requirement: Pack chain values
Uniform values read from a pack chain SHALL be clamped to a safe range per uniform, and a pack chain file larger than 64 KiB SHALL be ignored.

#### Scenario: Wild value
- **WHEN** `InverseAmount` is `1e300`
- **THEN** the amount used is 2
