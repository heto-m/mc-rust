## ADDED Requirements

### Requirement: Typed commands go to the server
A command typed in the chat box SHALL be sent to the server when submitted, whatever the local parse
shows; only clicked `run_command` events are checked locally.

#### Scenario: Local parse error
- **WHEN** the player submits `/setblock ~ ~ ~ stone` while the command box shows a parse error
- **THEN** the command is sent and the server's reply decides the outcome
