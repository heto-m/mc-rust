## MODIFIED Requirements

### Requirement: Status ping
The app SHALL ping each listed server with the status protocol and present MOTD, player counts, version, latency and favicon, treating the response as hostile. A server SHALL count as compatible when its reported protocol number is one of `mcr_protocol::version`'s supported protocols, and joining it SHALL use that protocol's wire.

#### Scenario: Oversized favicon
- **WHEN** a status response carries a favicon larger than the cap
- **THEN** the favicon is dropped and the rest of the status is shown

#### Scenario: Older supported server
- **WHEN** a listed server reports version 26.2 with protocol 776
- **THEN** it is shown as compatible and joining it uses the 26.2 translator

#### Scenario: Unsupported server
- **WHEN** a listed server reports protocol 770
- **THEN** it is shown as incompatible with its version name
