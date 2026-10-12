## ADDED Requirements

### Requirement: Leash ropes
A leashed entity SHALL draw a 24-segment rope from its leash offset to its holder's rope-hold position in alternating brown shades, and SHALL draw nothing for ropes longer than 64 blocks.

#### Scenario: Leashed mob
- **WHEN** an entity has a leash holder
- **THEN** pixels of the rope appear between the two attachment points

### Requirement: Thrown trident glint
A thrown trident with its foil flag set SHALL draw an additive enchantment glint layer over its model.

#### Scenario: Foil
- **WHEN** the synched foil flag is true
- **THEN** the plan holds a second, additive part using the item glint texture

### Requirement: Cushion and ominous item spawner
A cushion SHALL render its model in its dye colour facing its direction. An ominous item spawner SHALL render its item cluster growing over its first 50 ticks while spinning, fully lit.

#### Scenario: Cushion colour
- **WHEN** a cushion's colour is light blue
- **THEN** it uses `entity/cushion/light_blue_cushion`

### Requirement: Display billboards
Display entities SHALL orient by their billboard constraint: fixed uses the entity's rotation, vertical the camera yaw, horizontal the camera pitch and center both.

#### Scenario: Center
- **WHEN** the constraint is center
- **THEN** the display faces the camera

### Requirement: Ender dragon lean
The ender dragon SHALL turn by the yaw seven ticks ago and lean by ten degrees per block of height difference between five and ten ticks ago.

#### Scenario: Climbing
- **WHEN** the dragon gained 0.5 blocks between ten and five ticks ago
- **THEN** it leans 5 degrees

### Requirement: Elder guardian curse
The elder guardian curse event SHALL draw the guardian translucently in front of the camera for 30 ticks, fading in and out.

#### Scenario: Alpha
- **WHEN** the curse is halfway through
- **THEN** its alpha is 0.55
