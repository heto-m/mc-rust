# protocol-dispatch Specification

## Purpose
Packet-id dispatch enums, id tables, error context and byte-exactness against real vanilla traffic for protocol 777.

## Requirements
### Requirement: Dispatch enums
For every protocol state and direction the crate SHALL provide an enum with one variant per packet of that state and direction in the 26.3 packet report, with no missing and no extra packets.

#### Scenario: Coverage matches the report
- **WHEN** the packet names of an enum are compared with the data crate's names for the same state and direction
- **THEN** the sets are equal

### Requirement: Decode by id
Decoding SHALL take a packet id and payload, resolve the packet through a table built once from the version data, and fail unless the payload is fully consumed.

#### Scenario: Trailing bytes
- **WHEN** a payload has bytes left after the packet is decoded
- **THEN** decoding fails with an error naming the packet, state and direction

#### Scenario: Unknown id
- **WHEN** an id has no packet in the state and direction
- **THEN** decoding fails with an error naming the id, state and direction

### Requirement: Encode with id
Encoding a dispatch value SHALL return the packet id for the version and the payload bytes without the id.

#### Scenario: Round trip
- **WHEN** a decoded packet is encoded
- **THEN** the returned id and bytes equal the original id and payload

### Requirement: Byte-exact against real traffic
Every frame captured from a vanilla 26.3 server SHALL decode through the dispatch enums, consume all bytes, and re-encode to identical bytes.

#### Scenario: Captured frames
- **WHEN** each captured frame is decoded and re-encoded
- **THEN** the output equals the captured payload

### Requirement: Vanilla NBT key order for text components
Text components SHALL encode their NBT compound keys in the order a vanilla peer writes them (its hash-map iteration order), so re-encoded components equal the original bytes.

#### Scenario: Captured system chat
- **WHEN** a captured `system_chat` with a translatable component is decoded and re-encoded
- **THEN** the bytes are identical
