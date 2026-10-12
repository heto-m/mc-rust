## ADDED Requirements

### Requirement: Dialog checkboxes
Boolean inputs of server dialogs SHALL be `Checkbox` widgets, toggled by click, Enter or Space with the click sound, and submit the `on_true` or `on_false` value for their state.

#### Scenario: Keyboard toggle
- **WHEN** the checkbox of a dialog is focused and Space is pressed
- **THEN** it toggles and the click sound plays
