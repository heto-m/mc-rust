## ADDED Requirements

### Requirement: Automatic profile heap
A profile's JVM heap SHALL be automatic until the user moves its heap slider; an automatic heap SHALL show as "Auto" in the profile editor and leave the heap to the mod manager's automatic heap. A profile stored with `heap_mib` 2048 and no `heap_explicit` SHALL read as automatic.

#### Scenario: New profile
- **WHEN** a profile is created without touching the heap slider
- **THEN** it is saved without a heap and the mod set decides the heap at launch
