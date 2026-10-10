## ADDED Requirements

### Requirement: Connection lifecycle
The client SHALL connect to a server, perform handshake and login, complete configuration and enter play, handling compression, keep-alive, ping, transfer and disconnect.

#### Scenario: Offline login
- **WHEN** the server is in offline mode
- **THEN** the client sends `hello` with the name and offline UUID, acknowledges `login_finished`, finishes configuration and reaches the play state

#### Scenario: Compression
- **WHEN** the server sends `login_compression`
- **THEN** inbound frames are decompressed starting with the next frame and outbound compression is enabled

#### Scenario: Keep-alive
- **WHEN** a keep-alive or ping arrives in configuration or play
- **THEN** the matching response is sent immediately

#### Scenario: Disconnect
- **WHEN** the server sends a disconnect packet or closes the socket
- **THEN** the client enters a closed state and surfaces the reason as a `Component` or error

### Requirement: Configuration handling
The client SHALL answer known-packs, resource packs and cookie requests, store registry data in network order and tags, and support re-entering configuration from play.

#### Scenario: Known packs
- **WHEN** the server asks for known packs
- **THEN** the client replies with `minecraft:core` for the current version, or an empty list when configured so

#### Scenario: Re-configuration
- **WHEN** play receives `start_configuration`
- **THEN** the client acknowledges, handles configuration again and returns to play without dropping the connection

#### Scenario: Transfer
- **WHEN** a transfer packet arrives
- **THEN** the client reports the target host and port so the caller can reconnect with transfer intent
