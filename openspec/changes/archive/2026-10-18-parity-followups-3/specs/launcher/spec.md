## MODIFIED Requirements

### Requirement: Launcher view
The launcher view SHALL show the profile list, the active account and a Play button, drawn with mcr's UI. Play SHALL apply the selected profile (game dir, version jar, mods folder or vanilla, heap, resolution, account), record its last played time, and continue to the title screen, or in mod mode to Java's boot. Profiles SHALL be switched only in the launcher; the title screen SHALL lead back to it.

#### Scenario: Play a vanilla profile
- **WHEN** the user selects a profile without mods and clicks Play
- **THEN** the title screen shows and Java is not started, even if the game dir's mods folder has enabled mods

#### Scenario: Play a mod profile
- **WHEN** the selected profile names a mods folder with enabled mods and the user clicks Play
- **THEN** Java boots with those mods and its title screen is shown

#### Scenario: Unsupported profile
- **WHEN** the selected profile names a version mcr does not support
- **THEN** Play is inactive and the profile shows "not supported yet"

## ADDED Requirements

### Requirement: Back to the launcher
The native title screen SHALL have a Launcher button, and in mod mode the "mcr" menu SHALL offer Launcher while no world is open. Without a JVM in the process, mcr SHALL return to the launcher view in the same process. When a JVM was started, mcr SHALL shut Java down cleanly and start itself again with `--launcher` and the same `--data-dir`, keeping an offscreen run offscreen.

#### Scenario: Native round trip
- **WHEN** the user clicks Launcher on the native title screen
- **THEN** the launcher view shows in the same process and Play starts the selected profile again

#### Scenario: Mod mode
- **WHEN** the user picks Launcher in the "mcr" menu on Java's title screen
- **THEN** Java shuts down cleanly and a new mcr process shows the launcher with the same data dir
