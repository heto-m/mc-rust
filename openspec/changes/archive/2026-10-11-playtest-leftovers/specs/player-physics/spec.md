## ADDED Requirements

### Requirement: Eye and camera in fluid
The physics SHALL compute the eye-in-water state each tick like vanilla `EntityFluidInteraction`, using `getHeightForCamera` (full height for a source under a sturdy downward face, or for fluid with the same fluid above), and SHALL refresh it for passengers every tick. A shared camera check SHALL apply the same height rule to an arbitrary camera position.

#### Scenario: Full block under water
- **WHEN** the eye is in the top ninth of a water block that has water above
- **THEN** the eye is in water

#### Scenario: Riding
- **WHEN** the player rides a vehicle into water
- **THEN** its eye-in-water state follows the seat position
