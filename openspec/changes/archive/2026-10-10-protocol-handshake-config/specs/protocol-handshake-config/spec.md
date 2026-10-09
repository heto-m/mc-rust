## ADDED Requirements

### Requirement: Handshake, status and login packets
The system SHALL define every packet of the handshake, status and login states in both directions with
the exact field order and encodings of protocol 777.

#### Scenario: Handshake bytes
- **WHEN** an intention packet for protocol 777, host `localhost`, port 25565, status intent is encoded
- **THEN** the bytes are VarInt 777, the length-prefixed host, an unsigned big-endian short port and VarInt 1

#### Scenario: Round trip
- **WHEN** any handshake, status or login packet is encoded and decoded
- **THEN** the decoded value equals the original and no input remains

#### Scenario: Oversized field rejected
- **WHEN** a login hello name longer than 16 characters is decoded
- **THEN** decoding fails with a length error

### Requirement: Configuration and common packets
The system SHALL define every configuration packet in both directions, and SHALL define packets shared
between configuration and play exactly once.

#### Scenario: Shared packet reuse
- **WHEN** a keep-alive is needed in configuration or play
- **THEN** the same type is used for both states

#### Scenario: State-specific dialog encoding
- **WHEN** a dialog is sent in configuration versus play
- **THEN** configuration encodes the bare NBT and play encodes a registry holder

### Requirement: Profiles, known packs, registries and tags
The system SHALL model game profiles with signed properties, known packs, registry data entries with
optional NBT, and tag payloads keyed by registry.

#### Scenario: Profile properties
- **WHEN** a profile with a signed and an unsigned property is round-tripped
- **THEN** both properties and the presence of their signatures are preserved
