## ADDED Requirements

### Requirement: Complete component hashing
The client SHALL hash every persistent data component, including tool, equippable, consumable, trim, text events and nbt or object text, with vanilla's CRC32C `HashOps`.

#### Scenario: Every component variant
- **WHEN** a sample of each persistent component type is hashed
- **THEN** no component falls back to an unsupported hash

### Requirement: Menu quick move
The client SHALL route shift-clicks for every menu type like vanilla `quickMoveStack` and predict crafting-result pickups.

#### Scenario: Furnace fuel
- **WHEN** a fuel item is shift-clicked with a furnace open
- **THEN** it moves to the fuel slot without a server correction

### Requirement: Rideables
The client SHALL simulate or follow minecarts, striders, camels, pigs, happy ghasts and llama caravans as the vanilla client does.

#### Scenario: Pig boost
- **WHEN** the player rides a saddled pig holding a carrot on a stick
- **THEN** the boosted movement matches the server

### Requirement: Elytra flight
The client SHALL start, simulate and end fall flying with vanilla physics, including firework boost and riptide launch.

#### Scenario: Glide off a tower
- **WHEN** the player jumps from a height wearing an elytra and opens it
- **THEN** the server sends no corrective teleport
