# Spec Delta

## Purpose
Editor screens for operator blocks that a creative operator opens by right-clicking, each filled from the block entity the client holds and sending the vanilla serverbound packet.

## ADDED Requirements

### Requirement: Opening
The client SHALL raise a block editor event, instead of only sending the use packet, when a player who is a creative operator right-clicks a command block, structure block, jigsaw block, test block or test instance block, and a minecart editor event for a command block minecart.

#### Scenario: Not an operator
- **WHEN** a player without permission level 2 or creative mode right-clicks a command block
- **THEN** no editor event is raised

### Requirement: Command block editors
The system SHALL show the command, previous output with the track output toggle, and for blocks the mode (chain, repeat, impulse), conditional and always active controls, and Done or Enter SHALL send `set_command_block` or `set_command_minecart` with the edited values while Escape and Cancel send nothing.

#### Scenario: Edit
- **WHEN** the command is changed and Done is pressed
- **THEN** the packet carries the new command and the chosen mode and flags

### Requirement: Structure block editor
The system SHALL show the fields of the current mode (save, load, corner, data), accept only identifier characters in the structure name, clamp offsets to -48..48 and sizes to 0..48, parse numbers leniently, and send `set_structure_block` with the update type for Done, Save, Load and Detect.

#### Scenario: Clamp
- **WHEN** a size of 100 is entered and Save is pressed
- **THEN** the packet carries 48

### Requirement: Jigsaw and test editors
The system SHALL show the jigsaw fields with the joint type only for vertical jigsaws, the levels slider and keep jigsaws toggle, enabling Done and Generate only for valid identifiers, and the test block mode and message, and the test instance id, size, rotation and entities with its reset, save, run actions and the status the server reports.

#### Scenario: Invalid identifier
- **WHEN** the pool is not a valid identifier
- **THEN** Done and Generate are disabled

### Requirement: Labels
All labels SHALL come from the active language files.

#### Scenario: Language change
- **WHEN** the language is switched
- **THEN** the editor labels follow it
