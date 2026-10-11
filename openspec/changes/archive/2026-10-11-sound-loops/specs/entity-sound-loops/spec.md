## ADDED Requirements

### Requirement: Elytra wind
The client SHALL start a looping `item.elytra.flying` voice when the local player begins gliding, silent for 20 ticks, fading in over the next 20, with volume equal to the squared speed divided by 4 (clamped to 1) and pitch rising above volume 0.8, and stop it when gliding ends after the first 20 ticks.

#### Scenario: Fast glide
- **WHEN** the player glides at squared speed 4 for 40 ticks
- **THEN** the volume is 1 and the pitch 1.2

#### Scenario: Landing
- **WHEN** gliding stops after the first 20 ticks
- **THEN** the voice stops

### Requirement: Bee loops
The client SHALL give each of the 12 nearest bees a looping voice that follows it, using the aggressive loop while the bee's anger end time is in the future, with volume `1.2 * min(speed, 0.5)` and silence below a horizontal speed of 0.01.

#### Scenario: Anger changes
- **WHEN** a bee's anger end time passes the game time
- **THEN** its aggressive voice is replaced by the flying voice

#### Scenario: Crowd
- **WHEN** more than 12 bees are loaded
- **THEN** only the nearest 12 hold voices and the rest are stopped

#### Scenario: Bee removed
- **WHEN** a bee leaves the entity list
- **THEN** its voice stops

### Requirement: Guardian beam
On entity event 21 the client SHALL start a global looping `entity.guardian.attack` voice for that guardian whose volume is the square of the attack charge (ticks since the target changed over 80, or 60 for an elder guardian) and whose pitch is `0.7 + 0.5 * charge`, until the guardian is removed.

#### Scenario: Charging
- **WHEN** the guardian holds a target for 40 ticks
- **THEN** the volume is 0.25

#### Scenario: Target change
- **WHEN** the attack target changes
- **THEN** the charge restarts at zero
