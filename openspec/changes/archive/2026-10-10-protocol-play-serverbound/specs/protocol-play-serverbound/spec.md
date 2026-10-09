## ADDED Requirements

### Requirement: Packet coverage
Every serverbound play packet in the 26.3 packet report, except those shared with configuration, SHALL have a type implementing `Packet` with direction serverbound and the report's name.

#### Scenario: Names
- **WHEN** the `NAME` of each packet type is compared with the report
- **THEN** each equals `minecraft:` followed by the report name

### Requirement: Movement encoding
`move_player_pos`, `move_player_pos_rot`, `move_player_rot` and `move_player_status_only` SHALL encode their doubles and floats in vanilla order followed by one flags byte with bit 0 on-ground and bit 1 horizontal-collision.

#### Scenario: Position and rotation
- **WHEN** a position-and-rotation packet with on-ground set is encoded
- **THEN** the output is x, y, z as big-endian doubles, yaw and pitch as big-endian floats, then the byte `0x01`

### Requirement: Chat signing fields
`chat`, `chat_command_signed` and `chat_session_update` SHALL encode timestamps as epoch milliseconds, signatures as exactly 256 bytes, the last-seen update as offset VarInt, a 20-bit fixed bit set (3 bytes) and a checksum byte, and the session key data as expiry, length-prefixed public key and length-prefixed key signature.

#### Scenario: Unsigned chat
- **WHEN** a chat packet without a signature is encoded
- **THEN** the signature is a single `false` byte followed by the last-seen update

### Requirement: Container click
`container_click` SHALL encode container id, state id, slot, button, input mode, at most 128 changed slots keyed by short, and the carried item.

#### Scenario: Too many changed slots
- **WHEN** a click with more than 128 changed slots is decoded
- **THEN** decoding fails with `TooLong`

### Requirement: Low-precision vectors
`interact` SHALL encode its interaction location in the vanilla low-precision vector format.

#### Scenario: Zero vector
- **WHEN** a zero location is encoded
- **THEN** it occupies a single zero byte
