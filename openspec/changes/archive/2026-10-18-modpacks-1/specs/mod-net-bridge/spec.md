## ADDED Requirements

### Requirement: Configuration pings answered in order
While Java's mirror answers custom queries during configuration, Rust SHALL hold its answer to each configuration ping (at most 64) and send Java's pong when it arrives, so Java's custom payloads (Fabric's channel registration) reach the server first. When the mirror stops answering, Rust SHALL send the held pongs itself.

#### Scenario: Fabric server
- **WHEN** mcr in mod mode joins a Fabric server that requires Fabric API's channels
- **THEN** the server receives the channel registration before the pong and lets the player join

### Requirement: Undecodable play packets in mod mode
In mod mode, a play packet Rust cannot decode SHALL be skipped and counted (the first 16 logged) instead of disconnecting, since Java decodes and applies the same frame. Without mods the connection SHALL close as before.

#### Scenario: Modded packet payload
- **WHEN** a modded server sends a play packet whose payload holds modded data Rust cannot read
- **THEN** mcr stays connected and Java applies the packet
