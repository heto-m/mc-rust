## ADDED Requirements

### Requirement: Locator bar
When waypoints are tracked, the HUD SHALL draw the locator bar in place of the experience bar, except for 100 ticks after the experience progress changes. Each waypoint within 60 degrees of the view SHALL draw as a style sprite chosen by distance, tinted by its colour or a hash of its id, with an arrow when it lies above or below the view.

#### Scenario: Ahead
- **WHEN** a waypoint is straight ahead
- **THEN** its dot is at the middle of the bar

#### Scenario: Behind
- **WHEN** a waypoint is behind the player
- **THEN** it is not drawn

### Requirement: Tab list heads
On an encrypted connection the tab list SHALL draw each player's 8x8 head, with the hat layer when worn, before the name. Skins SHALL come from the `textures` property of the profile when it points at `textures.minecraft.net`, else from the default skin chosen by UUID.

#### Scenario: Foreign host
- **WHEN** the textures property names another host
- **THEN** nothing is downloaded and the default skin is used

### Requirement: Boss event world effects
Boss events with the darken flag SHALL darken the world lightmap, ramping up 0.05 and down 0.0125 per tick; those with the fog flag SHALL limit fog start to 10 and end to 96 blocks.

#### Scenario: Darken
- **WHEN** a darkening boss bar is present for 20 ticks
- **THEN** the darkening factor is 1

### Requirement: Debug charts
The debug overlay SHALL draw rolling frame time, tick time and ping charts of the latest 240 samples.

#### Scenario: Bounded history
- **WHEN** more than 240 samples were recorded
- **THEN** only the newest 240 bars are drawn
