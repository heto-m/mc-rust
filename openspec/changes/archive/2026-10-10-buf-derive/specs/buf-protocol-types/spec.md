## ADDED Requirements

### Requirement: Bit sets
`BitSet` SHALL encode as a VarInt-length array of big-endian longs; `FixedBitSet<BITS>` SHALL encode as `ceil(BITS/8)` bytes, least significant bit first.

#### Scenario: Fixed bit set bytes
- **WHEN** bit 9 of a `FixedBitSet<16>` is set
- **THEN** it encodes as `00 02`

### Requirement: Block position
`BlockPos` SHALL pack to an i64 with x in the top 26 bits, z in the middle 26 and y in the low 12, each signed.

#### Scenario: Negative coordinates
- **WHEN** a position with negative coordinates is packed and unpacked
- **THEN** the original coordinates are restored

### Requirement: Identifier
`Identifier` SHALL encode as a string, default the namespace to `minecraft`, and reject invalid characters on decode.

#### Scenario: Default namespace
- **WHEN** `stone` is decoded
- **THEN** it equals `minecraft:stone`

### Requirement: Bounded lists, angles and inline-or-registry values
`LengthPrefixed<T, MAX>` SHALL reject lengths over MAX. `Angle` SHALL be one byte of 1/256 turns. `IdOr<T>` SHALL encode VarInt 0 followed by an inline T, or registry id plus one.

#### Scenario: Registry id
- **WHEN** `IdOr::Id(4)` is encoded
- **THEN** the bytes are `05`
