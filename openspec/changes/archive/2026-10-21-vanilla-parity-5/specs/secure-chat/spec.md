## ADDED Requirements

### Requirement: Acknowledgement after the chat delay
With a chat delay, a player chat message SHALL be marked as processed when it leaves the delay queue, not
when it arrives.

#### Scenario: Delayed message
- **WHEN** the chat delay is 2 seconds and a signed message arrives
- **THEN** it joins the last-seen list only once it is shown
