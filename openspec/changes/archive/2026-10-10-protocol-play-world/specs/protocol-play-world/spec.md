# Spec Delta

## Purpose
Wire codecs for the clientbound play packets that describe entities, chunks, blocks, sounds, the world border and time in Minecraft 26.3 (protocol 777).

## ADDED Requirements

### Requirement: Packet identity and round trip
Every world and entity clientbound play packet SHALL expose its namespaced name and clientbound direction, and SHALL decode to a value equal to the one that was encoded, consuming the whole payload.

#### Scenario: Round trip
- **WHEN** any of these packets is encoded and then decoded
- **THEN** the decoded value equals the original and no input is left over

### Requirement: Exact field encodings
Field order and encodings SHALL match the vanilla codecs: VarInt versus fixed-width integers, optional values behind a boolean, enums as VarInt ordinals or bytes where vanilla does so, and chunk positions packed into one long.

#### Scenario: Block update bytes
- **WHEN** a block update at (1, 0, 0) with state 5 is encoded
- **THEN** the bytes are the packed position long followed by the VarInt 5

### Requirement: Section block updates
Section block updates SHALL carry a section position packed into one long and, per entry, a VarLong whose low 12 bits hold `x << 8 | z << 4 | y` (section-relative) and whose remaining bits hold the block state id.

#### Scenario: Entry layout
- **WHEN** state 5 at relative x=1, y=2, z=3 is encoded
- **THEN** the entry is the VarLong `5 << 12 | 1 << 8 | 3 << 4 | 2`

### Requirement: Low-precision vectors
Entity velocity SHALL use the low-precision vector encoding: zero is one byte, otherwise six bytes with a 15-bit quantised component each and a 2-bit scale, plus a VarInt scale extension when the scale exceeds 3.

#### Scenario: Zero and unit vectors
- **WHEN** the zero vector and the vector (1, -1, 0) are encoded
- **THEN** the bytes are `00` and the six-byte form with scale 1, and both decode back unchanged

### Requirement: Entity movement deltas
Relative entity movement SHALL pack the on-ground flag and step count into one VarInt, followed by either one linear delta of three shorts or one VarInt-tick plus three shorts per step.

#### Scenario: Stepped movement
- **WHEN** a movement with two steps is encoded
- **THEN** the properties VarInt is `2 << 1 | on_ground` and each step is a tick VarInt followed by three shorts

### Requirement: Deferred chunk and light payloads
Chunk-with-light, light-update and biome packets SHALL expose their fixed header fields typed and keep the registry-dependent payload as borrowed undecoded bytes that the world storage decoders accept unchanged.

#### Scenario: Chunk handed to world storage
- **WHEN** a chunk-with-light packet is decoded and its payload is passed to the world chunk and light decoders
- **THEN** both decoders succeed and consume the payload exactly

### Requirement: Debug subscription values
Debug value packets SHALL decode the subscription kinds whose value codec is simple and keep every other known kind as explicit opaque bytes. A subscription id outside the registry SHALL be rejected.

#### Scenario: Unknown subscription
- **WHEN** a debug event with subscription id 99 is decoded
- **THEN** decoding fails with an invalid-value error
