## ADDED Requirements

### Requirement: Cloud depth with improved transparency
With improved transparency on, fancy clouds SHALL use their own depth target so only the nearest cloud
face contributes to a pixel.

#### Scenario: Looking through a cloud
- **WHEN** the camera looks up through a fancy cloud with improved transparency on
- **THEN** only the nearest cloud face shades each pixel
