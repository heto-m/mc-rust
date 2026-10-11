## ADDED Requirements

### Requirement: Fabric runtime fetch
`fabric-fetch` SHALL download the Fabric Loader launcher profile for the target game version and pinned loader version, every library it lists, the LWJGL natives for the host platform from `version.json`, and the pinned test mods, into `.cache/fabric/`, verifying each file against its published checksum and skipping verified files.

#### Scenario: Checksum mismatch
- **WHEN** a downloaded library does not match its published checksum
- **THEN** the file is deleted and the command fails naming the artifact
