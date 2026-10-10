# sky-rendering Specification

## Purpose
Defines the vanilla sky dome, celestial bodies, stars, end sky and fancy clouds and the environment inputs that drive them.

## Requirements

### Requirement: Celestial environment
The environment SHALL derive sun, moon and star angles, moon phase, star brightness, sunrise colour, cloud colour and sky/fog colours from the day time and weather levels using the vanilla timeline values.

#### Scenario: Noon
- **WHEN** the day time is 6000
- **THEN** the sun angle is zero, stars are invisible and the sunrise alpha is zero

#### Scenario: Moon phase
- **WHEN** the total time advances by 24000 ticks
- **THEN** the moon phase advances by one step, wrapping after eight

#### Scenario: Rain greys the sky
- **WHEN** the rain level is one
- **THEN** the sky and cloud colours are blended towards grey and star brightness is zero

### Requirement: Sky drawing
The renderer SHALL draw the sunrise fan, sun, moon phase, stars, dark disc below the horizon and the end sky box behind all terrain.

#### Scenario: Stars at night
- **WHEN** star brightness is positive
- **THEN** about 1500 seeded star quads are drawn

### Requirement: Fancy clouds
The renderer SHALL draw extruded 12-block cloud cells from the cloud texture at the dimension cloud height, scrolled by game time and limited to the cloud range.

#### Scenario: Scrolling
- **WHEN** the game time increases
- **THEN** clouds drift along +X at 0.03 blocks per tick
