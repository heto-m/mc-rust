## ADDED Requirements

### Requirement: Class list per configuration
Every mod-mode session SHALL record the classes it loads through HotSpot's class-load log and, at shutdown, merge them into a versioned class list stored next to the AOT cache under the same key, so a changed mod, JDK, JVM option or bridge ABI starts a new list. The list SHALL be split into segments by the milestones Java reaches (the game's `main`, the game loop, the title screen, login) and keep classes the session did not load.

#### Scenario: Changed mod
- **WHEN** one mod jar's bytes change between two launches
- **THEN** the second launch records into a new list and preloads nothing

### Requirement: Bounded class lists
Class lists SHALL skip hidden, generated and JDK classes and stay bounded in classes, name length, segments and log bytes read. At most four other keys' lists SHALL be kept, the least recently used going first.

#### Scenario: Hostile log
- **WHEN** the class-load log holds more than 200,000 names, an over-long name or a marker number out of range
- **THEN** the list holds at most 200,000 valid names in at most 16 segments

#### Scenario: First session of a mod set
- **WHEN** a mod set without a class list boots to the title screen and quits
- **THEN** a list with the header `mcr-jvm-preload 1` exists for its key, holding the classes loaded after the game's `main` split by milestone

#### Scenario: Title-only session
- **WHEN** a session quits at the title screen and the previous list holds classes loaded after login
- **THEN** the merged list still holds those classes in their segment

### Requirement: Background preloading
When a class list exists for the current key and preloading is on, Java SHALL load the listed classes on a background daemon thread, starting at the game's `main`, without initializing them. A class SHALL wait until the session reached the milestone after which the previous session first loaded it.

#### Scenario: Second session of a mod set
- **WHEN** a mod set with a class list boots
- **THEN** Java reports per segment how many classes it preloaded, the title screen appears, and no listed class's static initializer runs because of preloading

#### Scenario: Login classes
- **WHEN** the previous session first loaded a class after the login packet
- **THEN** this session preloads it only after its own login packet

### Requirement: Preloading respects Mixin
Classes of Mixin and its registered extensions (which Mixin loads while holding its transformer lock), classes in mixin packages, and the targets and mod packages of mixin plugins whose `preApply` or `postApply` does anything SHALL be left to the game. Preloading SHALL stop for the session when the mixin configurations change after `main` or a class load fails with anything other than a missing class or a linkage error.

#### Scenario: Plugin that acts at transform time
- **WHEN** a mixin config's plugin has a non-empty `postApply`
- **THEN** that config's targets are not preloaded

#### Scenario: Transform on the render thread
- **WHEN** the render thread transforms a class and MixinExtras loads one of its classes that the list also holds
- **THEN** the preload thread holds no lock on that class, and neither thread deadlocks

### Requirement: Preloading never sticks
A session that preloads SHALL count as a strike until it ends cleanly without a `preload_stopped` report; a key with two strikes SHALL stop preloading. `MCR_JVM_PRELOAD=off` SHALL disable recording and preloading, and `MCR_JVM_PRELOAD=on` SHALL preload where the default would not.

#### Scenario: Hung preloading session
- **WHEN** two sessions that preload for a key are killed
- **THEN** the next session for that key records its classes but does not preload

#### Scenario: Off switch
- **WHEN** `MCR_JVM_PRELOAD=off` is set
- **THEN** Java starts without the class-load log option and without the preload property
