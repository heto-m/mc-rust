# ambient-sounds Specification

## Purpose
Client-side ambience: biome, underwater, block, fluid, weather and minecart sounds.

## Requirements

### Requirement: Biome ambient sounds
The client SHALL play the ambient loop of the attribute active at the player's position with 40 tick cross-fades, roll each additions entry every tick against its chance, and play the mood sound from a block within the search extent when the mood counter fills.

#### Scenario: Biome change
- **WHEN** the player moves between biomes with different loops
- **THEN** the old loop fades out and the new one fades in over 40 ticks

#### Scenario: Mood in the dark
- **WHEN** the sampled block has sky light 0 and block light 0 for the full tick delay
- **THEN** the mood sound plays in the sampled direction, offset further out

#### Scenario: Mood in daylight
- **WHEN** the sampled blocks have sky light above 0
- **THEN** the mood counter does not rise

#### Scenario: Hostile values
- **WHEN** an attribute has a negative tick delay, an enormous search extent or a chance above 1
- **THEN** the values are clamped to safe bounds

### Requirement: Underwater ambience
The client SHALL play enter and exit sounds when the local player's physics eye-in-water state changes (vanilla `isEyeInFluid(WATER)`, where a water block with water above is full height), a looping underwater ambience that fades in over 40 ticks and out twice as fast, and random additions with per-tick chances of 1%, 0.1% and 0.01% while underwater.

#### Scenario: Diving
- **WHEN** the eye enters water
- **THEN** the enter sound plays and the underwater loop begins

#### Scenario: Sinking to a pool bottom
- **WHEN** the player sinks through a deep pool and rests on its floor with the eye below the surface
- **THEN** the enter sound plays once and no exit sound plays, including while the eye passes the top ninth of fully submerged blocks

### Requirement: Block and fluid ambience
Each tick the client SHALL sample random positions within 16 and 32 blocks of the player and play the vanilla ambient cue of the block or fluid found: lit campfires, furnaces, blast furnaces, smokers and candles, fire, nether portals, charged respawn anchors, bubble columns, open eyeblossoms, dried ghasts, firefly bushes, potent sulfur, pale hanging moss, flowing water and lava with air above.

#### Scenario: Lit furnace
- **WHEN** a lit furnace is sampled
- **THEN** its crackle plays with probability 0.1

#### Scenario: Unlit furnace
- **WHEN** an unlit furnace is sampled
- **THEN** nothing plays

### Requirement: Weather sounds
The client SHALL play the rain sound at the rain ticker's cue position, using the quiet above variant when the position is more than one block above the camera and the camera is covered.

#### Scenario: Under a roof
- **WHEN** rain falls above a covered camera
- **THEN** the rain-above sound plays at volume 0.1 and pitch 0.5

### Requirement: Minecart sounds
The client SHALL loop a rolling sound at each minecart whose volume follows its horizontal speed and whose pitch ramps up while it moves, and an inside sound for the riding player that matches whether the player is underwater.

#### Scenario: Stopped cart
- **WHEN** a minecart is below 0.01 blocks per tick
- **THEN** its rolling sound is silent

### Requirement: Jukebox records
Level events 1010 and 1011 SHALL start and stop the record sound of the synced jukebox song at the block.

#### Scenario: Disc inserted
- **WHEN** level event 1010 arrives with a song id
- **THEN** the song's sound event plays at the block in the records category
