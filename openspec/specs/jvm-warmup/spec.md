# jvm-warmup Specification

## Purpose
Shortens mod-mode start-up with a JDK AOT cache per JVM configuration: recorded by a normal session, built
afterwards by a detached helper, used by later sessions, and never allowed to stop Java from starting.

## Requirements

### Requirement: AOT cache per configuration
Mod mode SHALL start Java from a JDK AOT cache when one exists for the current key. The key SHALL cover the JDK (release file and libjvm), the JVM options except `-D` properties, every class path entry by size and modification time, every mod jar by content, and the bridge ABI version. Caches SHALL live under `<game dir>/cache/jvm-aot`, never in the repository.

#### Scenario: Changed mod
- **WHEN** one mod jar's bytes change between two launches
- **THEN** the second launch computes a different key and does not use the first launch's cache

#### Scenario: Cache in use
- **WHEN** a cache exists for the current key
- **THEN** Java starts with `-XX:AOTCache` pointing at it and mcr logs that it uses it

### Requirement: Transparent training
Without a cache, a session SHALL record one (`-XX:AOTMode=record`) unless a recording waits for its build or a build runs. After a clean JVM shutdown mcr SHALL keep the recording and start a detached, lower-priority helper that builds the cache and publishes it by rename. A recording from an unclean shutdown SHALL be dropped.

#### Scenario: First and second launch
- **WHEN** a mod set is launched for the first time, the user quits normally and the helper finishes
- **THEN** the next launch with the same mod set uses the new cache

#### Scenario: Crash while recording
- **WHEN** Java does not stop cleanly in a recording session
- **THEN** the partial recording is deleted and no helper starts

### Requirement: Bounded and safe cache
An invalid, stale or refused cache SHALL never stop Java from starting. Where the platform allows, mcr SHALL detect a cache the JVM did not map and delete it. At most two caches SHALL be kept, a cache over 1 GiB SHALL be discarded, and a key whose recording or build failed three times SHALL stop recording. `MCR_JVM_AOT=off` SHALL disable the cache and recording.

#### Scenario: Corrupt cache
- **WHEN** the cache file for the current key is truncated
- **THEN** Java still reaches the title screen, and mcr deletes the file so the next session records again

#### Scenario: Eviction
- **WHEN** a third cache is built
- **THEN** the least recently used of the other two is deleted
