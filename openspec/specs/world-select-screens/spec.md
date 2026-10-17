# world-select-screens Specification

## Purpose
The world list, Create New World and Edit World screens, and the level.dat summary they show.

## Requirements

### Requirement: World list
The world list SHALL show the worlds of a `WorldStore` (folders under `<game dir>/saves` with a readable `level.dat`), newest last-played first, each with its icon or a placeholder, name, folder and date, and game mode, cheats and version. It SHALL offer Play Selected World, Create New World, Edit, Delete and Back. A double click or Enter SHALL play the selected world. At most 1000 worlds SHALL be listed.

#### Scenario: Play
- **WHEN** a world is selected and Play Selected World is clicked
- **THEN** `PlayWorld(folder)` is returned

#### Scenario: Damaged world
- **WHEN** a folder's `level.dat` is missing or not valid NBT
- **THEN** that folder is left out and the other worlds are listed

### Requirement: level.dat summary
The summary SHALL read `Data.LevelName`, `LastPlayed`, `GameType`, `allowCommands`, `Version.Name`, `DataVersion` and `difficulty_settings` (falling back to the old `Difficulty` and `hardcore` tags), bounding the file size, NBT depth and inflated size, and decode a 64×64 `icon.png` up to 1 MiB.

#### Scenario: Old world
- **WHEN** `level.dat` has `Difficulty: 3b` and `hardcore: 1b` but no `difficulty_settings`
- **THEN** the summary says hard and hardcore

### Requirement: Delete and rename
Delete SHALL ask for confirmation naming the world and then remove its folder, only inside the saves directory and never while its server runs. Edit SHALL rename the world's display name in `level.dat` (keeping `level.dat_old`), not its folder.

#### Scenario: Delete confirmed
- **WHEN** Delete is confirmed for world `Test`
- **THEN** its folder is removed and the list no longer shows it

### Requirement: Create New World screen
The screen SHALL offer world name, game mode (survival, hardcore, creative), difficulty (locked to hard in hardcore), allow cheats, world type (default, superflat, large biomes, amplified), seed, generate structures and bonus chest, and return `CreateWorld` with those settings. The folder name SHALL be derived from the name, replacing characters invalid on any platform and adding ` (n)` when taken.

#### Scenario: Name taken
- **WHEN** a world named `New World` is created and a `New World` folder exists
- **THEN** the new world's folder is `New World (1)`
