## ADDED Requirements

### Requirement: Struct derive
`Encode` and `Decode` SHALL be derivable for named, tuple and unit structs, with generics and borrowed lifetimes. Fields are encoded in declaration order with no padding.

#### Scenario: Borrowed struct round trip
- **WHEN** a struct with an `&'a str` field is encoded and decoded
- **THEN** the decoded value equals the original and borrows from the input

### Requirement: Enum derive
Enums SHALL be encoded as a discriminant followed by the variant fields. The discriminant defaults to VarInt and MAY be overridden to `u8`, `i32` or `varint`. Explicit discriminants SHALL be honored; others continue from the previous value.

#### Scenario: Unknown tag
- **WHEN** an enum is decoded from a discriminant matching no variant
- **THEN** decoding fails with `InvalidValue`

### Requirement: Field attributes
Fields SHALL accept the markers `varint` or `varlong` (i32/i64), `with = path` (module providing `encode` and `decode`), or `max_len = N` (string length limit in UTF-16 units).

#### Scenario: Max length
- **WHEN** a `max_len = 5` string field decodes a 6-character string
- **THEN** decoding fails with `TooLong`
