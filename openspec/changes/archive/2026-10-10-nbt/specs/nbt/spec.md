## ADDED Requirements

### Requirement: Owned NBT model
The library SHALL represent all thirteen NBT tag types, with compounds preserving insertion order and offering typed getters by key.

#### Scenario: Insertion order preserved
- **WHEN** keys "b", "a" are inserted into a compound and it is encoded then decoded
- **THEN** iteration yields "b" then "a"

#### Scenario: Typed getter
- **WHEN** a compound holds a string "text"
- **THEN** reading "text" as a string succeeds and reading it as an int yields nothing

### Requirement: Modified UTF-8
Strings and names SHALL be encoded and decoded as Java Modified UTF-8: NUL as 0xC0 0x80 and supplementary characters as surrogate pairs of three bytes each.

#### Scenario: Supplementary character round trip
- **WHEN** a string containing U+1F600 is written and read
- **THEN** it is encoded as six bytes and decodes to the same string

#### Scenario: Invalid sequence
- **WHEN** input contains an unpaired surrogate or truncated sequence
- **THEN** decoding fails with an error and does not panic

### Requirement: Disk and network formats
The library SHALL read and write disk NBT (named root compound, optionally gzip or zlib compressed, detected on read) and network NBT (root tag type byte followed directly by the payload with no name). A network root of type End SHALL denote an absent value.

#### Scenario: Network absent
- **WHEN** a single 0x00 byte is decoded as optional network NBT
- **THEN** the result is absent, and encoding absent yields 0x00

#### Scenario: Compressed disk file
- **WHEN** gzip-compressed named NBT is read
- **THEN** the root name and compound are returned

### Requirement: Hostile input limits
Decoding SHALL enforce a maximum nesting depth of 512 and a configurable byte quota (default 2 MiB for network), counting like vanilla, and SHALL NOT panic or allocate proportionally to declared lengths beyond the remaining input.

#### Scenario: Deep nesting
- **WHEN** input nests lists deeper than 512
- **THEN** decoding fails with a depth error

#### Scenario: Huge declared length
- **WHEN** an array declares 2^31-1 elements with few bytes remaining
- **THEN** decoding fails without large allocation

### Requirement: Borrowed reader
The library SHALL provide a zero-allocation reader that can skip values and look up keys in a compound, borrowing strings and arrays from the input.

#### Scenario: Extract one key
- **WHEN** a key is looked up in encoded compound bytes
- **THEN** its value is returned without constructing the owned model

### Requirement: SNBT
The library SHALL format values as SNBT and parse SNBT text into values, round-tripping the formatter output.

#### Scenario: Round trip
- **WHEN** a value is formatted to SNBT and parsed
- **THEN** the parsed value equals the original
