# modpacks Specification

## Purpose
Fetching CurseForge Fabric packs without an API key, placing each jar for mcr's client and for a server from one deny-list, and a Fabric test server for them.

## Requirements

### Requirement: Pack fetch without an API key
`cargo xtask modpack-fetch <zip>` SHALL read a CurseForge pack's `manifest.json` (bounded size and file count) and refuse packs for another game version, a loader other than Fabric, or another Fabric loader version than mcr pins. It SHALL resolve file names through cfwidget (sequential requests at least 600 ms apart, cached under `.cache/packs/cfwidget/`), download from the CurseForge CDN into `.cache/packs/files/<file id>/`, and check each download's size.

#### Scenario: Echoes of the Sift
- **WHEN** `cargo xtask modpack-fetch .cache/packs/echoes-of-the-sift-fabric-26.3.zip` runs twice
- **THEN** the second run downloads nothing and `.cache/packs/echoes-of-the-sift-fabric/mods` holds the same jars

### Requirement: Files cfwidget does not list
For a file id missing from cfwidget's list for its project, the fetcher SHALL take the name and size from the CurseForge website's file record, cached as `file-<id>.json`.

#### Scenario: Stale cfwidget list
- **WHEN** a manifest's file id is not in cfwidget's file list for its project
- **THEN** the fetcher downloads it under the name the file record gives

### Requirement: Pack overrides
Pack overrides SHALL be extracted with bounded entry count and size, never outside their folder, and installed into the game dir's `java/` folder without overwriting existing files unless forced. Shader packs SHALL be skipped.

#### Scenario: Second install
- **WHEN** overrides are installed into a game dir whose `options.txt` exists
- **THEN** that file is kept unless `--force` is given

### Requirement: Client and server sets from one policy
Each jar SHALL be placed for the client and for the server from its declared environment and `mod-policy.json`, the single deny-list. `blocked` and `unsupported` mods SHALL be dropped from the client; `server` mods (Lithium, C2ME, Krypton) and server-only jars SHALL run only on the server; client-only jars SHALL never run on the server.

#### Scenario: Sodium in a pack
- **WHEN** a pack lists Sodium and Reese's Sodium Options
- **THEN** both are dropped from the client with the policy's reason and neither runs on the server

### Requirement: Matching sides
A two-sided mod left out of the client SHALL be left off the server too, and a mod whose dependency was dropped SHALL be dropped. Jars for either side SHALL sit in one `mods/` folder, and `pack.json` SHALL record each file's placement and reason.

#### Scenario: Blocked two-sided mod
- **WHEN** a two-sided mod is blocked by the policy
- **THEN** it runs on neither side and `pack.json` says why

### Requirement: Fabric test server
`cargo xtask fabric-server --pack <slug>` SHALL run a Fabric dedicated server on loopback with the pack's server-side jars, offline mode, in a memory-capped scope, and SHALL refuse port 25565. It is a development tool and SHALL be documented as running without the client's egress fence.

#### Scenario: Port 25565
- **WHEN** `cargo xtask fabric-server --pack echoes-of-the-sift-fabric --port 25565` runs
- **THEN** it exits with an error before starting Java

### Requirement: Indexed pack zips
In mod mode, listing a directory of a zip resource pack SHALL read only the entries under that directory, from an
index of the zip's entries sorted by name built once per open zip, and SHALL hand them out in the zip's own order.

#### Scenario: Big texture pack
- **WHEN** Java's resource reload lists hundreds of directories of a pack zip with tens of thousands of entries
- **THEN** each listing reads only its own entries, and the loaded sprites and models are the same as without
  the index
