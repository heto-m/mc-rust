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

### Requirement: Create New World extras
Create New World SHALL offer a Customize button for superflat layers (a vanilla preset string, only for superflat), a Game Rules screen (26.3's rules by category, toggles and bounded number boxes) and a Data Packs screen (packs from `<game dir>/datapacks` with Open Pack Folder, plus the experiments), and carry their choices in `CreateWorld`.

#### Scenario: Turn off fall damage
- **WHEN** the user turns off Deal fall damage in Game Rules and creates the world
- **THEN** `CreateWorld` carries `fall_damage=false`

#### Scenario: Game rule changed
- **WHEN** keep inventory is turned on in Game Rules and Create is clicked
- **THEN** the returned settings hold `keep_inventory=true` and no unchanged rule

#### Scenario: Invalid number
- **WHEN** a number rule is set outside its bounds
- **THEN** the Game Rules screen marks it and Done is inactive

### Requirement: Re-Create
The world list SHALL offer Re-Create, which opens Create New World filled from the world: name, game mode, difficulty, cheats, seed, structures, bonus chest, world type with superflat layers, changed game rules, experiments and the world's data packs (reading `level.dat`, `data/minecraft/world_gen_settings.dat` and `data/minecraft/game_rules.dat`, or their pre-26 places, bounded like `level.dat`). A world whose generator is not one of the offered types SHALL be refused with a message.

#### Scenario: Re-create a seeded amplified world
- **WHEN** Re-Create is clicked for an amplified world with seed 42
- **THEN** Create New World shows the same name, amplified and seed 42, and creating it makes a new folder with ` (1)`

#### Scenario: Single biome world
- **WHEN** Re-Create is clicked for a single biome world
- **THEN** a message says it cannot be re-created and no create screen opens
