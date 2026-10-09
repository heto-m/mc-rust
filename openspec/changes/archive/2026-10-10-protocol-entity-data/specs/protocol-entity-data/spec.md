# Spec Delta

## Purpose
Wire codecs for particles, entity metadata lists and sound events as used by play packets in protocol 777.

## ADDED Requirements

### Requirement: Particle codec
A particle SHALL be encoded as its VarInt type id in `minecraft:particle_type` registry order followed by the options of that type, and ids at or above the registry size SHALL be rejected.

#### Scenario: Options-less particle
- **WHEN** a particle of a type without options is encoded
- **THEN** only its VarInt id is written and decoding returns the same particle

#### Scenario: Dust particle
- **WHEN** a dust particle with colour and scale is encoded
- **THEN** the id is followed by the colour as a 32-bit integer and the scale as a float

#### Scenario: Unknown type id
- **WHEN** a particle id beyond the registry is decoded
- **THEN** decoding fails with an error

#### Scenario: Registry order
- **WHEN** the generated registry report is available
- **THEN** every particle type name maps to the id the registry lists

### Requirement: Entity data list
An entity data list SHALL be a sequence of entries, each an index byte, a VarInt serializer id and the value, terminated by an index byte of 0xFF.

#### Scenario: Typical list
- **WHEN** a flags byte at index 0, an air supply VarInt at index 1 and a pose at index 6 are encoded
- **THEN** each entry is written with its serializer id and the list ends with 0xFF

#### Scenario: Unknown serializer
- **WHEN** an entry names a serializer id outside the registered range
- **THEN** decoding fails with an error

#### Scenario: Serializer order
- **WHEN** values of every serializer kind are encoded
- **THEN** each uses the serializer id given by 26.3 registration order

### Requirement: Entity data value codecs
Each serializer SHALL encode its value as the vanilla codec does, including optional values prefixed by a presence flag or encoded as zero, registry holders as VarInt ids, and enums as VarInt ordinals.

#### Scenario: Optional unsigned integer
- **WHEN** an absent value and the value 4 are encoded
- **THEN** they are written as VarInt 0 and VarInt 5

#### Scenario: Variant holder
- **WHEN** a painting variant is given inline
- **THEN** it is written as VarInt 0 followed by its fields, and a registry reference is written as id plus one

### Requirement: Sound event codec
A sound event SHALL be encoded as an identifier followed by an optional float range, and SHALL be usable inline inside an id-or-inline holder.

#### Scenario: Variable range
- **WHEN** a sound event without a range is encoded
- **THEN** the identifier is followed by a false flag

#### Scenario: Fixed range
- **WHEN** a sound event with range 16.0 is encoded
- **THEN** the identifier is followed by a true flag and the float
