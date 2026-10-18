## MODIFIED Requirements

### Requirement: Version list
The launcher SHALL list versions from Mojang's version manifest, fetched in the background and cached at `<data dir>/cache/version_manifest_v2.json` for offline starts, showing releases and, behind a toggle, snapshots. Versions not in `mcr_protocol::version::SUPPORTED_VERSIONS` SHALL be greyed out with "not supported yet" and SHALL NOT be selectable. The manifest SHALL be capped at 8 MiB and 4096 versions with validated ids. "Latest release" SHALL resolve to the newest supported release. Every supported version SHALL load its assets from the canonical version's client jar.

#### Scenario: Unsupported version
- **WHEN** the manifest lists 26.1.2, 26.2 and 26.3
- **THEN** 26.3 and 26.2 can be chosen and 26.1.2 is greyed with "not supported yet"

#### Scenario: Older version's assets
- **WHEN** a profile plays 26.2
- **THEN** its assets come from the 26.3 client jar

#### Scenario: Offline start
- **WHEN** the network is down and a cached manifest exists
- **THEN** the version list comes from the cache

#### Scenario: Snapshots hidden
- **WHEN** the snapshot toggle is off
- **THEN** no snapshot is listed
