## ADDED Requirements

### Requirement: Vanilla text wrapping
Wrapped text SHALL break, style and measure lines as vanilla's `StringSplitter` does, and chat SHALL indent every wrapped continuation line by one space.

#### Scenario: Chat continuation
- **WHEN** a chat message wraps onto a second line
- **THEN** the second line starts with a space, as in vanilla

#### Scenario: Font harness
- **WHEN** `cargo xtask parity font` runs
- **THEN** every case matches vanilla
