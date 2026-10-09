## ADDED Requirements

### Requirement: Layered asset lookup
The system SHALL resolve namespaced resources through an ordered stack of asset sources where later sources override earlier ones.

#### Scenario: Pack overrides vanilla
- **WHEN** the same resource path exists in the base source and in a later pack
- **THEN** reading that path returns the later pack's bytes

#### Scenario: Missing resource
- **WHEN** no source contains a path
- **THEN** the lookup reports absence without error

### Requirement: Zip and directory sources
The system SHALL read assets from zip archives (client jar, zipped packs) and from directories.

#### Scenario: Zip and directory agree
- **WHEN** the same content is exposed as a zip and as a directory
- **THEN** both yield identical bytes and identical file listings

### Requirement: Pack metadata and overlays
The system SHALL parse `pack.mcmeta` pack format information and apply overlay directories whose format range contains the target pack format.

#### Scenario: Matching overlay
- **WHEN** a pack declares an overlay whose range contains the target format
- **THEN** files under the overlay directory override the pack's base files

#### Scenario: Non-matching overlay
- **WHEN** the overlay range excludes the target format
- **THEN** its files are ignored
