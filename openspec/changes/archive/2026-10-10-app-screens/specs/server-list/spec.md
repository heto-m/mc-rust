## ADDED Requirements

### Requirement: Status ping
The app SHALL ping each listed server with the status protocol and present MOTD, player counts, version, latency and favicon, treating the response as hostile.

#### Scenario: Oversized favicon
- **WHEN** a status response carries a favicon larger than the cap
- **THEN** the favicon is dropped and the rest of the status is shown
