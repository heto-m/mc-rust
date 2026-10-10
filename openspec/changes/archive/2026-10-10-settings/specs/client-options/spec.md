## ADDED Requirements

### Requirement: Typed options with vanilla defaults

The system SHALL provide a typed `Options` value holding every client option of Minecraft 26.3 that
is written to `options.txt`, each initialized to its vanilla default and constrained to its vanilla
value range.

#### Scenario: Defaults match vanilla

- **WHEN** `Options::default()` is created
- **THEN** `renderDistance` is 12, `maxFps` is 120, `fov` is 70, `guiScale` is 0 (auto), `mouseSensitivity` is 0.5 and `chatOpacity` is 1.0

#### Scenario: Numeric values are clamped

- **WHEN** `options.txt` contains `maxFps:999`
- **THEN** the loaded `maxFps` is 260

### Requirement: Read vanilla options.txt

The system SHALL parse `options.txt` as UTF-8 lines of `key:value`, splitting on the first `:` only, and
SHALL apply each recognized key. Blank lines and lines without `:` SHALL be skipped with a warning.

#### Scenario: Import of a vanilla file

- **WHEN** a vanilla `options.txt` containing `renderDistance:16`, `particles:"decreased"`, `fov:0.5` and `key_key.jump:key.keyboard.space` is loaded
- **THEN** render distance is 16, particles is decreased, fov is 90 and the jump binding is the space key

#### Scenario: Malformed value falls back to default

- **WHEN** `options.txt` contains `renderDistance:far`
- **THEN** render distance keeps its default of 12 and a warning is recorded, and loading does not fail

#### Scenario: Arbitrary input never panics

- **WHEN** `options.txt` contains invalid UTF-8 sequences, very long lines or unknown enum names
- **THEN** loading returns successfully with defaults for the affected values

### Requirement: Write vanilla-format options.txt

The system SHALL write known options in vanilla's `key:value` format, with booleans and numbers as
plain text, named enums as quoted strings and legacy enums as integers, and SHALL preserve every
unrecognized key-value pair from the file it was loaded from.

#### Scenario: Unknown keys survive a round trip

- **WHEN** an `options.txt` containing `version:4325` and `someModKey:foo` is loaded and written back
- **THEN** the output still contains `version:4325` and `someModKey:foo` with their original values

#### Scenario: Round trip is stable

- **WHEN** an options file written by this crate is loaded and written again
- **THEN** the second output is identical to the first

### Requirement: Sound category volumes and model parts

The system SHALL store one volume per sound category (`soundCategory_<name>`, default 1.0) and one
visibility flag per player model part (`modelPart_<id>`, default true).

#### Scenario: Category volume

- **WHEN** `soundCategory_music:0.25` is loaded
- **THEN** the music volume is 0.25
