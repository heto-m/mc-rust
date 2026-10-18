## MODIFIED Requirements

### Requirement: Outbound filter
Rust SHALL forward a Java serverbound frame only if its phase tag equals Rust's current outbound phase and its packet class is on the forward list. Vanilla acknowledgements, keep-alives, movement and other packets Rust sends itself SHALL be dropped and counted.

#### Scenario: No duplicate keep-alive
- **WHEN** the server sends a keep-alive during play
- **THEN** the server receives exactly one keep-alive response

#### Scenario: Mod payload forwarded
- **WHEN** a mod sends a play-phase custom payload on a non-brand channel
- **THEN** the server receives it unchanged

#### Scenario: Difficulty on a remote server
- **WHEN** an operator on a remote server changes or locks the difficulty in Java's World Options
- **THEN** the server receives Java's `change_difficulty` or `lock_difficulty` packet
