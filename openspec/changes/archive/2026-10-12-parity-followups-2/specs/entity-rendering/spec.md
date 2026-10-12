## ADDED Requirements

### Requirement: Text display layout
A text display SHALL draw its text component wrapped to its line width (default 200, clamped to 1..1024), at most 64 lines, as glyph geometry lit by the entity's light. Lines SHALL be aligned left, right or centered by the alignment flags and SHALL have a shadow when the shadow flag is set.

#### Scenario: Wrapping and alignment
- **WHEN** a text display has a line width of 60 and a text longer than 60 pixels
- **THEN** it draws several lines, none wider than 60 pixels, each aligned by the alignment flags

#### Scenario: Hostile text
- **WHEN** the text has tens of thousands of line breaks or the line width is `i32::MAX`
- **THEN** at most 64 lines and 1024 pixels of width are laid out

### Requirement: Text display colours and depth
The background of a text display SHALL be its ARGB colour, or black at the background opacity option when the default-background flag is set, and SHALL span one pixel past the text on the left and top. Text alpha SHALL be the opacity byte, where values below 4 draw opaque like vanilla. The see-through flag SHALL draw without depth testing.

#### Scenario: See-through
- **WHEN** a see-through text display is behind a wall
- **THEN** it is drawn over the wall

#### Scenario: Default background
- **WHEN** the default-background flag is set and the background option is 0.5
- **THEN** the background is black with alpha 127

### Requirement: Text display orientation
A text display SHALL follow its billboard constraint and its interpolated transformation, and its text SHALL be readable from the display's +Z side.

#### Scenario: Center billboard
- **WHEN** the constraint is center
- **THEN** the text faces the camera

### Requirement: Display interpolation
Block, item and text displays SHALL interpolate their transformation between the previous and the current value over the entity's interpolation duration, starting after its start delay. Translation and scale SHALL interpolate linearly and rotations spherically. A change without a duration SHALL apply at once.

#### Scenario: Halfway
- **WHEN** the translation changes from 0 to 10 over 10 ticks and 5 ticks have passed
- **THEN** the displayed translation is 5

### Requirement: Text display interpolation
Text displays SHALL interpolate the opacity as a signed byte and the background per ARGB channel over the same duration. Transformation components SHALL be clamped to 64 and non-finite values SHALL be treated as 0.

#### Scenario: Hostile transformation
- **WHEN** the scale is infinite and a rotation is NaN
- **THEN** the displayed transformation is finite

### Requirement: Display brightness and range
A display with a brightness override SHALL use its block and sky light instead of the world's. A display SHALL be drawn only within its view range times 64 blocks times the entity distance scaling, and SHALL skip frustum culling when its width or height is 0.

#### Scenario: Override
- **WHEN** the brightness override is block 15, sky 0
- **THEN** the display is lit with block light 15 and sky light 0
