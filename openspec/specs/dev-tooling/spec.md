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
