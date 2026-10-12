## ADDED Requirements

### Requirement: Checkbox
The checkbox widget SHALL draw a 17 by 17 box from the `widget/checkbox` sprites (plain, highlighted, selected, selected highlighted) followed by its label four pixels to the right with a shadow. A left click or Enter or Space on the focused checkbox SHALL toggle it and report the new state.

#### Scenario: Toggle
- **WHEN** an unchecked checkbox is clicked
- **THEN** it reports `Toggled(true)` and draws the selected sprite
