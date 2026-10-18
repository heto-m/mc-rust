## ADDED Requirements

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
