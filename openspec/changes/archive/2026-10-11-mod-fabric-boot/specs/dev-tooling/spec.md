## ADDED Requirements

### Requirement: Fabric runtime fetch
`fabric-fetch` SHALL download the Fabric Loader launcher profile for the target game version and pinned loader version, every library it lists, the vanilla libraries and the LWJGL natives for the host platform from `version.json`, and the pinned test mods, into `.cache/`, verifying each file against its published checksum and skipping verified files. It SHALL extract the host natives into one directory and write a launch manifest (`.cache/fabric/launch.json`) with cache-relative paths.

#### Scenario: Checksum mismatch
- **WHEN** a downloaded library does not match its published checksum
- **THEN** the file is deleted and the command fails naming the artifact

#### Scenario: Launch manifest
- **WHEN** `fabric-fetch` completes
- **THEN** `.cache/fabric/launch.json` lists the main class, the loader and vanilla class path, the game jar, the natives directory, the asset index and the test mods

### Requirement: Java module dependencies
A module MAY declare dependencies in `deps.txt`: `module <name>` for another module's jar, `fabric-launch` for the class path in the Fabric launch manifest. Dependencies SHALL build first, and their hashes and the launch manifest SHALL be part of the module hash.

#### Scenario: Dependency change
- **WHEN** a module that another module depends on changes
- **THEN** both modules are rebuilt, the dependency first

## MODIFIED Requirements

### Requirement: Java module build
`java-build` SHALL compile each module under `java/` with the provisioned JDK 25 (`javac --release 25`), package it with `jar` into `.cache/java/<module>.jar` together with the module's `resources/` tree and `MANIFEST.MF`, and skip modules whose source hash is unchanged. It SHALL never package files from Mojang jars.

#### Scenario: Incremental build
- **WHEN** `java-build` runs twice without source changes
- **THEN** the second run compiles nothing and reports every module as up to date
