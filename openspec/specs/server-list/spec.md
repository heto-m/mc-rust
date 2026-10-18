# server-list Specification

## Purpose
Multiplayer server list in vanilla servers.dat (gzip NBT), including hidden entries and resource pack policy.

## Requirements

### Requirement: Read and write servers.dat

The system SHALL read and write `servers.dat` in vanilla's layout: a gzip-compressed NBT root compound with
a `servers` list whose entries carry `name`, `ip`, an optional base64 `icon`, an optional
`acceptTextures` boolean, an optional `acceptedCodeOfConduct` int and a `hidden` boolean.

#### Scenario: Round trip

- **WHEN** a server list with two entries, one with an icon and one hidden, is written and read back
- **THEN** the same names, addresses, icons and hidden flags are returned in the same order

#### Scenario: Missing fields default

- **WHEN** an entry has no `name` or `acceptTextures`
- **THEN** the name is empty and the resource pack status is prompt

#### Scenario: Hostile input is bounded

- **WHEN** `servers.dat` is nested beyond the NBT depth limit or larger than the uncompressed limit
- **THEN** reading fails with an error and does not panic

### Requirement: Hidden entries

Entries with `hidden` set to true SHALL be kept on disk and marked hidden, and SHALL NOT be treated as
ordinary visible servers by the list accessor.

#### Scenario: Visible list

- **WHEN** a list contains one visible and one hidden server
- **THEN** the visible list contains only the visible server

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
