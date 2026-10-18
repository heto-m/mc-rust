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

#### Scenario: Difficulty on a remote server
- **WHEN** an operator on a remote server changes or locks the difficulty in Java's World Options
- **THEN** the server receives Java's `change_difficulty` or `lock_difficulty` packet

### Requirement: Chat through Rust
Chat messages and commands sent from Java SHALL be handed to Rust as text and sent by Rust's chat session, never forwarded as raw frames, so they are signed by Rust's keys when a session exists. Java's chat acknowledgements and session updates SHALL stay dropped; Rust alone acknowledges. With an online account Java SHALL fetch Mojang's services key set so its chat HUD verifies sessions and messages like vanilla, while holding no profile keys itself.

#### Scenario: Command from a Java screen
- **WHEN** a Java chat screen submits `/time query day`
- **THEN** Rust sends a chat command packet with that command and the server replies to the player

#### Scenario: Signed chat from a Java screen
- **WHEN** Java's chat screen sends "hello" while Rust has a chat session
- **THEN** Rust sends a `chat` packet signed with its session and Java never sends a chat frame

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

### Requirement: Server packs in Java
When mcr has applied a set of server resource packs, Java SHALL load the same pack files from mcr's downloads and SHALL drop them when they are popped, without downloading them again, prompting, or answering the server.

#### Scenario: Pack applied
- **WHEN** the server pushes a pack and mcr downloads and applies it
- **THEN** Java's resources include that pack and the server receives only mcr's answers

#### Scenario: Pack popped
- **WHEN** the server pops the pack
- **THEN** Java reloads without it

### Requirement: Quiet proxy refusals
Requests that the loopback web proxy refuses because their host is not allow-listed SHALL produce at most one short log line per logger and no stack trace or error toast.

#### Scenario: Update check
- **WHEN** a mod's update check asks for `api.modrinth.com`
- **THEN** the log shows one line naming the logger and the allow-list, and no exception

### Requirement: Local actions reach Java's player
Rust SHALL post the actions vanilla predicts on the client without a server echo (item drops, hand swings, mining progress and attack-strength resets) to Java in order with the packet copies, and Java SHALL apply each to its `LocalPlayer` with vanilla's own calls.

#### Scenario: Drop with Q
- **WHEN** the player drops one item from a selected stack of 16
- **THEN** Java's selected stack holds 15 and Java's hand swings

#### Scenario: Mining
- **WHEN** Rust starts mining a block
- **THEN** Java's main hand swings and Java's tutorial receives the destroy progress

### Requirement: Movement keys for Java
Rust's player state SHALL carry the held movement keys, and Java SHALL report them as its input's key presses to the tutorial and mods while Java's own movement vector stays zero.

#### Scenario: Movement tutorial
- **WHEN** a new player walks with the movement keys and turns the mouse
- **THEN** Java's movement tutorial step completes and its toast goes away
