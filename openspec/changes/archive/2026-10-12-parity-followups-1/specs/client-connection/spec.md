## ADDED Requirements

### Requirement: Code of conduct
The client SHALL show a server's code of conduct before acknowledging it, unless the server's entry already holds the hash of that exact text. Declining SHALL disconnect. The displayed text SHALL be stripped of control characters and `§` and bounded.

#### Scenario: First visit
- **WHEN** the server sends a code of conduct whose hash is not remembered
- **THEN** the acknowledgement is held until the player accepts, and declining closes the connection

#### Scenario: Remembered
- **WHEN** the entry stores the hash of the received text
- **THEN** the client acknowledges without showing a screen

### Requirement: Spectated camera
`set_camera` SHALL make the view follow the named entity when it is known and is not the player, send no movement while it does, hide that entity from the first-person view and return to the player on sneak, respawn or when the entity is removed.

#### Scenario: Unknown entity
- **WHEN** `set_camera` names an entity the client does not have
- **THEN** the view stays on the player

### Requirement: Server-provided chat completions
`custom_chat_completions` SHALL add, remove or replace a bounded list used to complete the last word of plain chat.

#### Scenario: Add and complete
- **WHEN** the entries "Alpha" and "alpine" are added and the player types "hello al"
- **THEN** both are offered for the word starting at index 6

### Requirement: Transient blocks
`add_transient_block` SHALL draw the block at its position for one second, replacing an earlier one at the same position, with a bounded count.

#### Scenario: Expiry
- **WHEN** 20 client ticks pass
- **THEN** the block is no longer drawn

### Requirement: Server icon
A valid 64x64 PNG icon in `server_data` SHALL be stored in the connected server's list entry.

#### Scenario: Invalid icon
- **WHEN** the icon is not a 64x64 PNG
- **THEN** the entry is left unchanged

### Requirement: Head rotation smoothing
`rotate_head` SHALL turn the head over three ticks instead of snapping.

#### Scenario: One tick
- **WHEN** the head target is 90 degrees away
- **THEN** after one tick the head has turned 30 degrees
