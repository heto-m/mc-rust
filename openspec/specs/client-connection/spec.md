# client-connection Specification

## Purpose
Drives the protocol state machine from handshake to play, including re-configuration, transfer and disconnect.

## Requirements

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

### Requirement: Code of conduct
The client SHALL show a server's code of conduct before acknowledging it, unless the server's entry already holds the hash of that exact text. Declining SHALL disconnect. The displayed text SHALL be stripped of control characters and `§` and bounded.

#### Scenario: First visit
- **WHEN** the server sends a code of conduct whose hash is not remembered
- **THEN** the acknowledgement is held until the player accepts, and declining closes the connection

#### Scenario: Remembered
- **WHEN** the entry stores the hash of the received text
- **THEN** the client acknowledges without showing a screen

### Requirement: Spectated camera
`set_camera` SHALL make the view follow the named entity when it is known and is not the player, send no movement while it does, hide that entity from the first-person view and return to the player on sneak, respawn or when the entity is removed.

#### Scenario: Unknown entity
- **WHEN** `set_camera` names an entity the client does not have
- **THEN** the view stays on the player

### Requirement: Server-provided chat completions
`custom_chat_completions` SHALL add, remove or replace a bounded list used to complete the last word of plain chat.

#### Scenario: Add and complete
- **WHEN** the entries "Alpha" and "alpine" are added and the player types "hello al"
- **THEN** both are offered for the word starting at index 6

### Requirement: Transient blocks
`add_transient_block` SHALL draw the block at its position for one second, replacing an earlier one at the same position, with a bounded count.

#### Scenario: Expiry
- **WHEN** 20 client ticks pass
- **THEN** the block is no longer drawn

### Requirement: Server icon
A valid 64x64 PNG icon in `server_data` SHALL be stored in the connected server's list entry.

#### Scenario: Invalid icon
- **WHEN** the icon is not a 64x64 PNG
- **THEN** the entry is left unchanged

### Requirement: Head rotation smoothing
`rotate_head` SHALL turn the head over three ticks instead of snapping.

#### Scenario: One tick
- **WHEN** the head target is 90 degrees away
- **THEN** after one tick the head has turned 30 degrees

### Requirement: Wire protocol choice
`ClientConfig` SHALL choose the wire: canonical, a fixed supported protocol, or auto. Auto SHALL first ping the server's status (bounded response, the connect timeout) and use the wire for its reported protocol, refusing an unsupported protocol with an error naming the server's version. Every frame the client sends or receives SHALL pass through the chosen wire.

#### Scenario: Direct connect to an older server
- **WHEN** the client connects with auto to a 26.2 server
- **THEN** it pings, picks the 26.2 translator and reaches the play state

#### Scenario: Unsupported server version
- **WHEN** the client connects with auto to a server reporting protocol 770
- **THEN** the connection fails before login with a message naming the server's version

### Requirement: App wire selection
The app SHALL use the protocol from the server list's ping when it has one, auto for direct connects and transfers, and canonical for its own local server.

#### Scenario: Singleplayer
- **WHEN** the app joins its managed local server
- **THEN** the canonical wire is used without a status probe
