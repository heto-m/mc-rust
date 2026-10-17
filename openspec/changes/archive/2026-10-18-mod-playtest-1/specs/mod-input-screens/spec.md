## MODIFIED Requirements

### Requirement: Single text source with an IME
While an IME composition is in progress, text SHALL reach Java only through IME commits, never also through key press text. Text input being enabled without a composition SHALL NOT suppress key press text.

#### Scenario: Composed character
- **WHEN** the IME commits a character that winit also reports as key text
- **THEN** Java receives it once

#### Scenario: Text input without an input method
- **WHEN** the compositor enables text input with no input method running and the user types "hi" into Java's chat
- **THEN** Java's chat box contains "hi"
