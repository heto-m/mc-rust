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
