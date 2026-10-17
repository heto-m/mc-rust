## ADDED Requirements

### Requirement: Local actions reach Java's player
Rust SHALL post the actions vanilla predicts on the client without a server echo (item drops, hand swings, mining progress and attack-strength resets) to Java in order with the packet copies, and Java SHALL apply each to its `LocalPlayer` with vanilla's own calls.

#### Scenario: Drop with Q
- **WHEN** the player drops one item from a selected stack of 16
- **THEN** Java's selected stack holds 15 and Java's hand swings

#### Scenario: Mining
- **WHEN** Rust starts mining a block
- **THEN** Java's main hand swings and Java's tutorial receives the destroy progress

### Requirement: Movement keys for Java
Rust's player state SHALL carry the held movement keys, and Java SHALL report them as its input's key presses to the tutorial and mods while Java's own movement vector stays zero.

#### Scenario: Movement tutorial
- **WHEN** a new player walks with the movement keys and turns the mouse
- **THEN** Java's movement tutorial step completes and its toast goes away
