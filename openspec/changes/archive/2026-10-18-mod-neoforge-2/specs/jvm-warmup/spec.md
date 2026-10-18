## MODIFIED Requirements

### Requirement: AOT cache per configuration
Mod mode SHALL start Java from a JDK AOT cache when one exists for the current key, under Fabric and NeoForge alike. The key SHALL cover the JDK (release file and libjvm), the JVM options except `-D` properties, every class path entry by size and modification time, every mod jar by content, and the bridge ABI version. Under NeoForge it SHALL also cover the patched game jar by size and modification time and the NeoForge version, which FML reads from its library directory instead of the class path. Caches SHALL live under `<game dir>/cache/jvm-aot`, never in the repository.

#### Scenario: Changed mod
- **WHEN** one mod jar's bytes change between two launches
- **THEN** the second launch computes a different key and does not use the first launch's cache

#### Scenario: Cache in use
- **WHEN** a cache exists for the current key
- **THEN** Java starts with `-XX:AOTCache` pointing at it and mcr logs that it uses it

#### Scenario: Re-patched NeoForge game jar
- **WHEN** `cargo xtask neoforge-fetch` rewrites the patched client jar between two NeoForge launches
- **THEN** the second launch computes a different key

### Requirement: Preloading respects Mixin
Classes of Mixin and its registered extensions (which Mixin loads while holding its transformer lock), classes in mixin packages, the targets and mod packages of mixin plugins whose `preApply` or `postApply` does anything, and under NeoForge the packages of FML's class processors SHALL be left to the game. Under NeoForge the preloader SHALL load through FML's transforming class loader. Preloading SHALL stop for the session when the mixin configurations change after `main`, when FML's class processors cannot be read, or when a class load fails with anything other than a missing class or a linkage error.

#### Scenario: Plugin that acts at transform time
- **WHEN** a mixin config's plugin has a non-empty `postApply`
- **THEN** that config's targets are not preloaded

#### Scenario: Transform on the render thread
- **WHEN** the render thread transforms a class and MixinExtras loads one of its classes that the list also holds
- **THEN** the preload thread holds no lock on that class, and neither thread deadlocks

#### Scenario: NeoForge session with a class list
- **WHEN** a NeoForge mod set with a class list boots
- **THEN** Java reports per segment how many classes it preloaded through FML's loader, and no class processor's package is among them
