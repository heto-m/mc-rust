## ADDED Requirements

### Requirement: Version tables
`cargo xtask version-tables --version <old>` SHALL read the old and the canonical version's `generated/reports` (`packets.json`, `registries.json`, `blocks.json`) and write the old version's translation tables into `crates/mcr-protocol/src/version/`, failing with the missing path when a report is absent and when an id does not fit the table format.

#### Scenario: Missing reports
- **WHEN** `.cache/versions/26.2/generated/reports` does not exist
- **THEN** the command fails and tells the user to run `cargo xtask fetch` and `cargo xtask datagen` for 26.2
