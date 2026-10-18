## ADDED Requirements

### Requirement: Animated cutout mips
The cutout coverage of an animated sprite SHALL be measured and rescaled over its whole frame strip, as vanilla mips the strip, so every frame of a mip level gets the same alpha scale.

#### Scenario: Animated kelp at a distance
- **WHEN** distant kelp plants animate through all their frames at the third and fourth mip levels
- **THEN** no frame loses all of its coverage, so the kelp does not blink
