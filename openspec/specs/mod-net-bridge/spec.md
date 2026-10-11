# mod-net-bridge Specification

## Purpose
Mirror the server stream Rust owns into the embedded Java client, so mods see a normal vanilla connection while Rust keeps the socket, the protocol state, the local player and every acknowledgement.

## Requirements

### Requirement: Rust owns the protocol
In mod mode, Rust SHALL keep sole ownership of the socket, encryption, compression, login and the protocol phase. Java SHALL never open a network connection and SHALL receive frames only through the bridge.

#### Scenario: Join with mods
- **WHEN** `mcr --mods` joins a vanilla server
- **THEN** exactly one TCP connection to the server exists and it is owned by Rust's I/O threads

### Requirement: Clientbound copy
Every clientbound frame Rust decodes SHALL be copied in order, tagged with its phase, to Java, except the login encryption request and the login compression packet. Copying SHALL never block Rust's reader thread. Overflow beyond the spill limit SHALL disconnect only the Java side.

#### Scenario: Java world follows the server
- **WHEN** Java has been joined for 30 seconds on a vanilla server
- **THEN** Java's loaded chunk set, the block states at 20 sampled positions and the entity id set equal Rust's

#### Scenario: Java stalls
- **WHEN** Java stops draining and the spill limit is exceeded
- **THEN** Java receives a disconnect and Rust stays connected

### Requirement: Outbound filter
Rust SHALL forward a Java serverbound frame only if its phase tag equals Rust's current outbound phase and its packet class is on the forward list. Vanilla acknowledgements, keep-alives, movement and other packets Rust sends itself SHALL be dropped and counted.

#### Scenario: No duplicate keep-alive
- **WHEN** the server sends a keep-alive during play
- **THEN** the server receives exactly one keep-alive response

#### Scenario: Mod payload forwarded
- **WHEN** a mod sends a play-phase custom payload on a non-brand channel
- **THEN** the server receives it unchanged

### Requirement: Chat through Rust
Chat messages and commands sent from Java SHALL be handed to Rust as text and sent by Rust's chat session, never forwarded as raw frames.

#### Scenario: Command from a Java screen
- **WHEN** a Java chat screen submits `/time query day`
- **THEN** Rust sends a chat command packet with that command and the server replies to the player

### Requirement: Player shadow
Rust's local player state SHALL overwrite Java's `LocalPlayer` position, rotation, velocity, on-ground, pose and selected slot every tick. Java SHALL not apply its own movement input or send movement packets.

#### Scenario: Positions agree
- **WHEN** the Rust player walks 20 blocks
- **THEN** Java's `LocalPlayer` position after the next tick equals Rust's to within 1e-6

### Requirement: Disconnect propagation
A disconnect on either side SHALL close the other: a server or Rust disconnect SHALL disconnect Java's connection with the reason, and a Java-initiated disconnect SHALL make Rust close the socket.

#### Scenario: Java disconnect button
- **WHEN** the user presses Disconnect in Java's pause screen
- **THEN** Rust closes the server connection
