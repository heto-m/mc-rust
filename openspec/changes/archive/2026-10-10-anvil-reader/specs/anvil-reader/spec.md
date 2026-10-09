## ADDED Requirements

### Requirement: Region file reading
The reader SHALL parse region files with an 8 KiB header of sector offsets and timestamps and decode each chunk payload according to its compression byte: gzip, zlib, none, LZ4. A custom compression id SHALL be rejected, chunks flagged as external SHALL be read from their sibling `.mcc` file, and every malformed table or payload SHALL produce an error and never panic or read out of bounds.

#### Scenario: Round trip per compression
- **WHEN** a chunk is written to a region file using each of gzip, zlib, none and LZ4
- **THEN** reading it back yields the identical NBT

#### Scenario: Absent chunk
- **WHEN** a chunk's offset entry is zero
- **THEN** reading it reports that the chunk does not exist

#### Scenario: External chunk
- **WHEN** a chunk's compression byte has the external flag set and a sibling `c.<x>.<z>.mcc` file exists
- **THEN** the payload is read from that file

#### Scenario: Corrupt sector table
- **WHEN** an offset points past the end of the file, overlaps the header, or a stored length exceeds its sectors
- **THEN** reading that chunk fails with an error while other chunks remain readable

#### Scenario: Custom compression
- **WHEN** a chunk uses compression id 127
- **THEN** reading it fails with an unsupported-compression error

### Requirement: Chunk conversion
The reader SHALL convert chunk NBT into a `ChunkColumn`: block-state and biome palettes with packed data, block and sky light, heightmaps and block entities. Palette entries SHALL map name and properties to global state ids; unknown entries SHALL become air and be counted as warnings. Chunks whose `Status` is below the configured minimum SHALL be skipped.

#### Scenario: Palette mapping
- **WHEN** a section palette contains `minecraft:stone` and `minecraft:oak_stairs` with properties
- **THEN** blocks resolve to the matching global state ids

#### Scenario: Unknown block
- **WHEN** a palette names a block not in the registry
- **THEN** those positions are air and the unknown count increases

#### Scenario: Partial chunk
- **WHEN** a chunk's status is `minecraft:noise` and the minimum is `minecraft:full`
- **THEN** it is not returned

#### Scenario: Data version
- **WHEN** a chunk's `DataVersion` differs from the supported version
- **THEN** it is still loaded and a warning is logged

### Requirement: Level summary
The reader SHALL parse `level.dat` into a summary of name, last played, game type, hardcore, commands allowed, data version, version name, spawn position and player position, and list the worlds in a directory ordered by last played, newest first, skipping directories that are not worlds.

#### Scenario: Listing
- **WHEN** a saves directory holds two worlds and a stray folder
- **THEN** the two worlds are listed newest first

### Requirement: World access
A `World` SHALL locate dimension region directories in both the legacy and the `dimensions/` layouts, load a whole region, and load all chunks within a radius of a chunk in parallel.

#### Scenario: Radius load
- **WHEN** chunks around a center within radius r are requested
- **THEN** every existing, sufficiently generated chunk in the square is returned with its position
