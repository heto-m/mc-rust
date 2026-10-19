## ADDED Requirements

### Requirement: Options are applied and sent
Every vanilla option mcr stores SHALL take effect, the options screens SHALL offer vanilla's options, and
the client information packet SHALL carry language, chat visibility, chat colours, model parts, main hand,
server listing and particles, resent when they change.

#### Scenario: Left-handed
- **WHEN** the player sets the main hand to left
- **THEN** a client information packet with main hand left is sent

### Requirement: Global and debug keys
F3 combos, F3+F4 game mode switcher, F2 screenshots, F11 fullscreen, social interactions (P), quick actions
(G) and the smooth camera key SHALL work as in vanilla, with the F3 overlay toggled on release when no combo
was used.

#### Scenario: Hitboxes
- **WHEN** the player presses F3+B
- **THEN** entity hitboxes toggle and the debug overlay does not
