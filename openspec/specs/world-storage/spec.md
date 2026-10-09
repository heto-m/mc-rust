# world-storage Specification

## Purpose
Compact client-side chunk storage that decodes the 26.3 chunk and light wire formats for the renderer and physics.

## Requirements

### Requirement: Paletted container wire format
The system SHALL decode and encode block-state (4096 entries) and biome (64 entries) containers in the 26.3 network format: a bits-per-entry byte, a palette (a single value id for 0 bits, a length-prefixed id list for indirect, nothing for direct), then a data array whose length is derived from the bit width and carries no length prefix, with entries never spanning longs.

#### Scenario: Single-value container
- **WHEN** a container with 0 bits and one palette id is decoded
- **THEN** every position reads that id and no data longs are consumed

#### Scenario: Indirect and direct containers
- **WHEN** block containers with 4 to 8 bits, or more than 8 bits, are decoded
- **THEN** the former resolve ids through the palette and the latter use values as global ids, and re-encoding then decoding yields identical entries

#### Scenario: Bit width selection
- **WHEN** a block container is read with 1 to 4 bits it is treated as 4 bits, and a biome container accepts 1 to 3 bits as indirect
- **THEN** wider widths are direct with the width of the global registry

#### Scenario: Hostile input
- **WHEN** a palette index is outside the palette, a global id exceeds the registry size, or the input is truncated
- **THEN** decoding returns an error and never panics

### Requirement: Entry indexing
The system SHALL index section blocks by y, then z, then x (`(y << 8) | (z << 4) | x`) and biomes by `(y << 4) | (z << 2) | x`, and provide O(1) get, set with palette growth, and bulk unpack into a flat array.

#### Scenario: Set grows the palette
- **WHEN** a block is set that is not in the palette of a full-width container
- **THEN** the container widens and all existing entries are preserved

### Requirement: Chunk payload decoding
The system SHALL decode the chunk data struct (heightmaps, section buffer, block entities as raw data) and the light data struct (four section bitsets, sky and block 2048-byte nibble arrays) for a dimension described by min_y and height.

#### Scenario: Full column
- **WHEN** a payload for a dimension of N sections is decoded
- **THEN** N sections are produced, light is stored for N+2 sections, and the buffer must be consumed exactly

### Requirement: Chunk store
The system SHALL store columns by chunk position, answer world-coordinate block lookups, apply block updates, and expose a drainable set of dirty sections.

#### Scenario: Update marks dirty
- **WHEN** a block on a section border is changed
- **THEN** that section and the adjacent loaded section are reported dirty once and are cleared by draining
