## ADDED Requirements

### Requirement: Lock modifier bits
The modifier state sent to Java SHALL include the SDL Caps Lock and Num Lock bits, toggled by each press of those keys.

#### Scenario: Caps Lock
- **WHEN** the user presses Caps Lock once
- **THEN** Java's key events carry the Caps Lock modifier until it is pressed again

### Requirement: Single text source with an IME
While an IME is active, text SHALL reach Java only through IME commits, never also through key press text.

#### Scenario: Composed character
- **WHEN** the IME commits a character that winit also reports as key text
- **THEN** Java receives it once

### Requirement: Held keys survive a screen closing
Movement keys still held when a Java screen closes SHALL keep moving the player without a re-press.

#### Scenario: Closing the inventory while walking
- **WHEN** the user holds W, a Java screen opens and closes while W stays down
- **THEN** the player walks forward again when the screen closes

### Requirement: One resource-pack prompt
In mod mode, a server resource pack SHALL produce exactly one prompt and exactly one response packet to the server.

#### Scenario: Server pushes a pack
- **WHEN** the server pushes a resource pack while Java is attached
- **THEN** only mcr's prompt shows and the server receives a single answer

### Requirement: Native menu layout
The native "mcr" menu SHALL draw on an opaque panel with its title above its buttons, hiding Java's controls beneath it.

#### Scenario: Pause screen
- **WHEN** the user presses the "mcr" button on Java's pause screen
- **THEN** the menu's title and buttons are drawn without Java's buttons showing through
