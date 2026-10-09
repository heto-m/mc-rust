# server-address-resolution Specification

## Purpose
Turn a user-entered server address into a host and port, following _minecraft._tcp SRV records when no port is given.

## Requirements

### Requirement: Address parsing
The resolver SHALL parse `host`, `host:port`, `[v6]` and `[v6]:port`, defaulting the port to 25565.

#### Scenario: Default port
- **WHEN** parsing `example.com`
- **THEN** the host is `example.com`, the port is 25565, and no explicit port is recorded

### Requirement: SRV redirection
When no explicit port is given and the host is not an IP literal, the resolver SHALL query `_minecraft._tcp.<host>` SRV over UDP using the system nameservers and use the best-priority target. Any failure SHALL fall back silently to the original host and port.

#### Scenario: SRV answer
- **WHEN** the nameserver answers with an SRV record
- **THEN** its target and port replace the host and port

#### Scenario: Lookup failure
- **WHEN** the query times out or returns no SRV record
- **THEN** the original host and default port are used with no error
