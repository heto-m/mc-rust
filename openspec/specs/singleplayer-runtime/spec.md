# singleplayer-runtime Specification

## Purpose
Provisioning what a local world needs at runtime: Mojang's server jar (SHA-1 checked), a Java 25 runtime, progress feedback and the player's EULA agreement.

## Requirements

### Requirement: Server jar from Mojang's manifest
The client SHALL download the server jar for its version at runtime: the version manifest, then the version JSON (SHA-1 checked against the manifest entry), then the jar (SHA-1 and size checked against the version JSON). It SHALL be stored in `<game dir>/versions/<version>/server.jar`, never in the repository, and reused offline on later launches. Every download SHALL be HTTPS and size-bounded.

#### Scenario: First launch
- **WHEN** no server jar exists and the network is reachable
- **THEN** the jar is downloaded with progress, verified and kept for later launches

#### Scenario: Corrupt download
- **WHEN** the downloaded jar's SHA-1 differs from the version JSON
- **THEN** the partial file is deleted and the world does not start, with an error shown

### Requirement: Java runtime for the server
The server SHALL run on Java 25 or newer found the way the mod layer finds it (`MCR_JAVA_HOME`, the dev JDK, `JAVA_HOME`, then `java` on the path). Without one, the client SHALL install Mojang's `java-runtime` component named by the version JSON into `<game dir>/runtime/<component>`, checking each file's SHA-1, rejecting paths that leave the target and bounding file count and total size.

#### Scenario: Reused JDK
- **WHEN** `JAVA_HOME` points at a JDK 25
- **THEN** the server starts with that JDK's `java` and nothing is downloaded

#### Scenario: Hostile runtime manifest
- **WHEN** a runtime manifest lists a file at `../escape`
- **THEN** the install fails and nothing is written outside the runtime directory

### Requirement: Provisioning feedback
Preparing a world SHALL show a progress screen whose status names the current step (downloading the server with a percentage, downloading Java, starting, preparing the spawn area with its percentage) and SHALL show a failure screen with the reason when a step fails. Cancel SHALL stop any server already started.

#### Scenario: Offline without a jar
- **WHEN** the jar is missing and the manifest cannot be fetched
- **THEN** a failure screen explains that the server could not be downloaded

### Requirement: EULA acknowledgement
Before the first local server starts, the client SHALL ask the player to agree to the Minecraft EULA (with a button opening it) and SHALL remember the answer in `<game dir>/eula.txt`. Only after agreement SHALL it write `eula=true` into the server directory.

#### Scenario: First world
- **WHEN** the player plays a world for the first time and agrees
- **THEN** the world starts, and later worlds start without asking again
