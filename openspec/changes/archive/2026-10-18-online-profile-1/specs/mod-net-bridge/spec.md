## MODIFIED Requirements

### Requirement: Chat through Rust
Chat messages and commands sent from Java SHALL be handed to Rust as text and sent by Rust's chat session, never forwarded as raw frames, so they are signed by Rust's keys when a session exists. Java's chat acknowledgements and session updates SHALL stay dropped; Rust alone acknowledges. With an online account Java SHALL fetch Mojang's services key set so its chat HUD verifies sessions and messages like vanilla, while holding no profile keys itself.

#### Scenario: Command from a Java screen
- **WHEN** a Java chat screen submits `/time query day`
- **THEN** Rust sends a chat command packet with that command and the server replies to the player

#### Scenario: Signed chat from a Java screen
- **WHEN** Java's chat screen sends "hello" while Rust has a chat session
- **THEN** Rust sends a `chat` packet signed with its session and Java never sends a chat frame
