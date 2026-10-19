## ADDED Requirements

### Requirement: Java's packs predicted at startup
In mod mode, mcr SHALL build its first resource stack with the resource packs that Java's `options.txt` selects, before Java reports its selection. It SHALL read at most 256 `file/` entries, SHALL reject names that contain path separators or are `.` or `..`, and SHALL keep only packs that resolve inside the game dir. mcr SHALL compare its stack with Java's selection only after Java's first render report, and SHALL rebake only when they differ.

#### Scenario: Selection unchanged
- **WHEN** Java's options select 16 packs and Java reports the same 16
- **THEN** mcr loads its resources once at startup and does not reload them at the title screen

#### Scenario: Hostile options
- **WHEN** Java's options list `file/../../etc` or a symlink that leaves the game dir
- **THEN** that entry is ignored
