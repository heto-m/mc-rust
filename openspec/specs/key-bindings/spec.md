# key-bindings Specification

## Purpose
Vanilla key mappings and input keys, with lookup, conflict detection, and toggle or hold state for sneak, sprint, attack and use.

## Requirements

### Requirement: Vanilla key mappings with defaults

The system SHALL define every vanilla 26.3 key mapping (movement, inventory, multiplayer, miscellaneous,
creative, spectator, the nine hotbar slots and the debug keys), each with its vanilla default input.

#### Scenario: Every default parses

- **WHEN** the default binding of each key mapping is serialized and parsed back
- **THEN** the parsed input equals the default

#### Scenario: Stored names

- **WHEN** the forward key is bound to W
- **THEN** it is written as `key_key.forward:key.keyboard.w`

### Requirement: Input key names

The system SHALL represent keyboard keys by their GLFW code and vanilla `key.keyboard.*` name, and mouse
buttons as `key.mouse.*`. An unrecognized name SHALL yield the unbound input rather than an error.

#### Scenario: Mouse button name

- **WHEN** `key_key.attack:key.mouse.left` is loaded
- **THEN** attack is bound to mouse button 1

#### Scenario: Unknown name unbinds

- **WHEN** `key_key.jump:key.keyboard.nonsense` is loaded
- **THEN** jump is unbound and a warning is recorded

### Requirement: Action lookup and conflicts

The system SHALL map a pressed input to all key mappings bound to it, and SHALL report every input bound
to more than one mapping as a conflict. Unbound inputs SHALL NOT conflict.

#### Scenario: Lookup

- **WHEN** the space key is pressed with the default bindings
- **THEN** the lookup returns the jump mapping

#### Scenario: Conflict

- **WHEN** both forward and jump are bound to space
- **THEN** the conflict list contains space with both mappings

### Requirement: Toggle and hold semantics

For sneak, sprint, attack and use, the system SHALL honor the matching `toggleCrouch`, `toggleSprint`,
`toggleAttack` and `toggleUse` options. With toggle enabled a press flips the logical state and the release
has no effect; with hold semantics the logical state follows the physical state.

#### Scenario: Toggle sneak

- **WHEN** `toggleCrouch` is true and sneak is pressed twice with a release in between
- **THEN** the logical sneak state is on after the first press and off after the second

#### Scenario: Hold sprint

- **WHEN** `toggleSprint` is false and sprint is pressed and released
- **THEN** the logical sprint state is on while held and off after release
