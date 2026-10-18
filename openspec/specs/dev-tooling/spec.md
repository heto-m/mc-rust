# dev-tooling Specification

## Purpose
Provides the `cargo xtask` developer commands that fetch, verify, generate and run the reference Minecraft
artifacts (jars, JDK, data generator output, decompiled sources, vanilla server) under the repository's
gitignored `.cache/` and `run/` directories.

## Requirements

### Requirement: Artifact layout
All downloaded and generated outputs SHALL be written under `<repo>/.cache/`, using the layout
`versions/<ver>/{version.json,client.jar,server.jar,datagen/,generated/,src/}` and `tools/{jdk25/,vineflower-1.12.0.jar}`.

#### Scenario: Version directory paths
- **WHEN** the layout is resolved for version `26.3`
- **THEN** the client jar path is `<repo>/.cache/versions/26.3/client.jar` and the datagen directory is `<repo>/.cache/versions/26.3/datagen`

#### Scenario: Repository root
- **WHEN** xtask resolves the repository root
- **THEN** it uses the parent of the xtask crate directory, independent of the current working directory

### Requirement: Idempotent commands
Every work-producing command SHALL skip work whose outputs already exist and verify, and SHALL redo the work when `--force` is passed.

#### Scenario: Re-running fetch with verified outputs
- **WHEN** `fetch` runs and `client.jar` and `server.jar` exist with matching SHA-1
- **THEN** no download happens and the command reports the artifacts as up to date

#### Scenario: Forced redo
- **WHEN** a command runs with `--force`
- **THEN** its outputs are regenerated even if they already exist

### Requirement: Fetch verified version artifacts
`fetch` SHALL resolve the requested version from the piston-meta version manifest (latest release when `--version` is omitted), download its `version.json`, `client.jar` and `server.jar`, and fail if a downloaded jar's SHA-1 differs from the value declared in `version.json`.

#### Scenario: Default version
- **WHEN** `fetch` runs without `--version`
- **THEN** the latest release listed in the manifest is used

#### Scenario: Unknown version
- **WHEN** `--version` names an id absent from the manifest
- **THEN** the command fails with an error naming the id

#### Scenario: Checksum mismatch
- **WHEN** a downloaded jar's SHA-1 does not match the declared hash
- **THEN** the file is not kept and the command fails

### Requirement: Java runtime provisioning
`jdk` SHALL use a system `java` when its reported major version is 25 or higher; otherwise it SHALL install Eclipse Temurin JDK 25 for the current OS and architecture into `.cache/tools/jdk25/` with the top-level archive directory stripped.

#### Scenario: Suitable system Java
- **WHEN** `java -version` reports 25 or higher
- **THEN** no download happens

#### Scenario: Portable JDK install
- **WHEN** no suitable system Java exists and `.cache/tools/jdk25/bin/java` is absent
- **THEN** the Temurin archive for the host platform is downloaded and extracted into `.cache/tools/jdk25/`

### Requirement: Data generator run
`datagen` SHALL run the server jar's bundler main class `net.minecraft.data.Main` with `--reports --server --output ../generated` from the `datagen/` working directory, write combined output to `datagen.log`, and fail when the process exits non-zero.

#### Scenario: Successful generation
- **WHEN** the generator exits with status 0
- **THEN** `generated/` contains the reports and the command succeeds

#### Scenario: Failed generation
- **WHEN** the generator exits non-zero
- **THEN** the command fails and reports the exit status and the log path

### Requirement: Decompilation
`decompile` SHALL download Vineflower 1.12.0 from Maven Central when absent, and decompile `net/minecraft` and `com/mojang` classes from the client jar into `src/`.

#### Scenario: Decompile run
- **WHEN** `decompile` runs for a version with a fetched client jar
- **THEN** Vineflower runs with a 6 GB heap and writes sources to `.cache/versions/<ver>/src/`

### Requirement: Vanilla server run
`server` SHALL run the server jar in `run/server-<ver>/` with `online-mode=false`, the given port, and a small view distance, writing these settings into `server.properties`. It SHALL write `eula.txt` with `eula=true` only when the environment variable `MCR_ACCEPT_EULA` equals `1`; otherwise it SHALL exit with a non-zero status and a message linking https://aka.ms/MinecraftEULA. Stdin SHALL be forwarded to the server, and `--nogui` SHALL be passed through.

#### Scenario: EULA not accepted
- **WHEN** `MCR_ACCEPT_EULA` is unset and `eula.txt` does not contain `eula=true`
- **THEN** the command exits non-zero without starting the server and prints the EULA URL

#### Scenario: EULA accepted by environment
- **WHEN** `MCR_ACCEPT_EULA=1` is set
- **THEN** `eula.txt` contains `eula=true` before the server starts

### Requirement: Setup pipeline
`setup` SHALL run `fetch`, `jdk`, `datagen` and `decompile` in that order for the same version, stopping at the first failure.

#### Scenario: Stop on failure
- **WHEN** `datagen` fails during `setup`
- **THEN** `decompile` does not run

### Requirement: Java module build
`java-build` SHALL compile each module under `java/` with the provisioned JDK 25 (`javac --release 25`), package it with `jar` into `.cache/java/<module>.jar` together with the module's `resources/` tree and `MANIFEST.MF`, and skip modules whose source hash is unchanged. It SHALL never package files from Mojang jars.

#### Scenario: Incremental build
- **WHEN** `java-build` runs twice without source changes
- **THEN** the second run compiles nothing and reports every module as up to date

### Requirement: Java module dependencies
A module MAY declare dependencies in `deps.txt`: `module <name>` for another module's jar, `fabric-launch` for the class path in the Fabric launch manifest. Dependencies SHALL build first, and their hashes and the launch manifest SHALL be part of the module hash.

#### Scenario: Dependency change
- **WHEN** a module that another module depends on changes
- **THEN** both modules are rebuilt, the dependency first

### Requirement: Fabric runtime fetch
`fabric-fetch` SHALL download the Fabric Loader launcher profile for the target game version and pinned loader version, every library it lists, the vanilla libraries and the LWJGL natives for the host platform from `version.json`, and the pinned test mods, into `.cache/`, verifying each file against its published checksum and skipping verified files. It SHALL extract the host natives into one directory and write a launch manifest (`.cache/fabric/launch.json`) with cache-relative paths.

#### Scenario: Checksum mismatch
- **WHEN** a downloaded library does not match its published checksum
- **THEN** the file is deleted and the command fails naming the artifact

#### Scenario: Launch manifest
- **WHEN** `fabric-fetch` completes
- **THEN** `.cache/fabric/launch.json` lists the main class, the loader and vanilla class path, the game jar, the natives directory, the asset index and the test mods

### Requirement: Soak harness
`cargo xtask soak` SHALL start a vanilla server with a fresh world in its own run directory (port 25567 by default, refusing to start while another `server.jar` runs) and run `mcr --soak` offscreen in its own memory-capped scope for the requested duration (20 minutes by default), sampling mcr's RSS and thread count from `/proc` and its printed GPU counters into a CSV every 10 seconds.

#### Scenario: Another server running
- **WHEN** a `server.jar` is already running
- **THEN** the soak refuses to start

### Requirement: Soak churn
Through the server console the soak SHALL churn entity waves, far teleports and dimension changes, `/reload`, chat, titles, boss bars, scoreboards and teams, items with components, signs and banners, weather and time, particles and sounds, and kicks and transfers.

#### Scenario: A cycle
- **WHEN** one 60-second churn cycle has run
- **THEN** it summoned and killed a wave, removed the previous boss bar, objective and team, and teleported the player

### Requirement: Soak verdict
The soak SHALL fail when the RSS or GPU memory slope after warm-up exceeds the limit (1 MiB per minute by default), when the thread count grows, when mcr exits early, or, with `--mods`, when the JVM heap after collection grows faster than the limit.

#### Scenario: Leak detected
- **WHEN** RSS grows 2 MiB per minute after warm-up
- **THEN** the soak fails and names the slope

#### Scenario: Flat run
- **WHEN** RSS, GPU memory and threads stay level after warm-up
- **THEN** the soak passes and writes `samples.csv` and `summary.txt`

### Requirement: Offscreen soak mode
`mcr --connect <addr> --soak <secs>` SHALL play offscreen for the given time, rejoin the last server three seconds after a session ends, respawn a dead player, and print a `soak-stats` line every 10 seconds with wgpu's live buffer and texture bytes, bind groups, the entity renderer's live texture layers, and the entity and chunk counts.

#### Scenario: Kicked during a soak
- **WHEN** the server kicks the player
- **THEN** mcr joins again and the next stats line counts one more join

### Requirement: Version tables
`cargo xtask version-tables --version <old>` SHALL read the old and the canonical version's `generated/reports` (`packets.json`, `registries.json`, `blocks.json`) and write the old version's translation tables into `crates/mcr-protocol/src/version/`, failing with the missing path when a report is absent and when an id does not fit the table format.

#### Scenario: Missing reports
- **WHEN** `.cache/versions/26.2/generated/reports` does not exist
- **THEN** the command fails and tells the user to run `cargo xtask fetch` and `cargo xtask datagen` for 26.2
