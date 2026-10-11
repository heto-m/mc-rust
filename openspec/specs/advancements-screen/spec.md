# advancements-screen Specification

## Purpose
Advancements screen showing the server-sent advancement trees, with layout bounded against hostile data.

## Requirements

### Requirement: Advancements screen

The client SHALL open the advancements screen with the advancements key, show one tab per root advancement with a background, and draw each tab's tree with frames, connectors, progress and tooltips.

#### Scenario: Opening a tab

- **WHEN** the player opens the screen and selects a tab
- **THEN** the client sends `SeenAdvancements` for that tab and shows its tree

### Requirement: Bounded advancement layout

The client SHALL treat non-finite positions as zero, clamp positions to a finite range, cap the nodes per tab, stop parent walks at a fixed depth, and truncate tooltip text before wrapping.

#### Scenario: Hostile positions

- **WHEN** an advancement has a NaN, infinite or huge position
- **THEN** the tab bounds stay finite and the screen still renders
