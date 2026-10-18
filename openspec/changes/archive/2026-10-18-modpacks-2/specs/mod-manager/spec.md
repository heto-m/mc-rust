## ADDED Requirements

### Requirement: Automatic heap
Without a chosen heap, the JVM heap SHALL follow the enabled client mod count: 2.3 times an estimated live heap of 510 MiB plus 4.8 MiB a mod, in 256 MiB steps, at least 2048 MiB, and at most 12288 MiB, three quarters of system memory and system memory less 4608 MiB.

#### Scenario: Small set
- **WHEN** 12 client mods are enabled and no heap was chosen
- **THEN** Java starts with 2048 MiB

#### Scenario: Large set
- **WHEN** 102 client mods are enabled and no heap was chosen
- **THEN** Java starts with 2304 MiB

### Requirement: Chosen heap
A heap chosen on the command line, in the launcher profile or in the Mods screen SHALL win over the automatic one, capped at 12288 MiB and three quarters of system memory. A settings file holding 2048 without `heap_explicit` SHALL read as automatic.

#### Scenario: Old settings file
- **WHEN** `mcr-mods.json` holds `"jvm": {"heap_mib": 2048}` and 102 client mods are enabled
- **THEN** Java starts with the automatic 2304 MiB

### Requirement: Heap shown
The heap Java starts with and how it was chosen SHALL be logged at boot and shown in the Mods screen, marked "(auto)" when automatic.

#### Scenario: Mods screen
- **WHEN** the Mods screen opens with 102 client mods and no chosen heap
- **THEN** its heap slider reads "JVM heap: 2304 MiB (auto)"
