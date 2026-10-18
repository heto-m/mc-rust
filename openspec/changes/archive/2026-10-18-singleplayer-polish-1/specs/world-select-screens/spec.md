## MODIFIED Requirements

### Requirement: Create New World screen
The screen SHALL offer world name, game mode (survival, hardcore, creative), difficulty (locked to hard in hardcore), allow cheats, world type (default, superflat, large biomes, amplified), seed, generate structures and bonus chest, a Customize button for superflat layers (a vanilla preset string), a Game Rules screen (26.3's rules by category, toggles and bounded number boxes), and a Data Packs screen (packs from `<game dir>/datapacks` with Open Pack Folder, plus the experiments), and return `CreateWorld` with those settings. The folder name SHALL be derived from the name, replacing characters invalid on any platform and adding ` (n)` when taken.

#### Scenario: Name taken
- **WHEN** a world named `New World` is created and a `New World` folder exists
- **THEN** the new world's folder is `New World (1)`

#### Scenario: Game rule changed
- **WHEN** keep inventory is turned on in Game Rules and Create is clicked
- **THEN** the returned settings hold `keep_inventory=true` and no unchanged rule

#### Scenario: Invalid number
- **WHEN** a number rule is set outside its bounds
- **THEN** the Game Rules screen marks it and Done is inactive

## ADDED Requirements

### Requirement: Re-Create
The world list SHALL offer Re-Create, which opens Create New World filled from the world: name, game mode, difficulty, cheats, seed, structures, bonus chest, world type with superflat layers, changed game rules, experiments and the world's data packs (reading `level.dat`, `data/minecraft/world_gen_settings.dat` and `data/minecraft/game_rules.dat`, or their pre-26 places, bounded like `level.dat`). A world whose generator is not one of the offered types SHALL be refused with a message.

#### Scenario: Re-create a seeded amplified world
- **WHEN** Re-Create is clicked for an amplified world with seed 42
- **THEN** Create New World shows the same name, amplified and seed 42, and creating it makes a new folder with ` (1)`

#### Scenario: Single biome world
- **WHEN** Re-Create is clicked for a single biome world
- **THEN** a message says it cannot be re-created and no create screen opens
