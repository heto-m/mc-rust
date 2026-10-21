# gameplay-correctness Specification

## Purpose
Client-side gameplay that must match vanilla so the server never corrects the client: component hashing, click prediction, digging, item use, vehicles, edits and state.

## Requirements

### Requirement: Component hashing
The client SHALL hash item components with vanilla's CRC32C `HashOps` so `container_click` hashed stacks match the server's.

#### Scenario: Named item click
- **WHEN** a stack with a custom name is clicked
- **THEN** the sent hash equals the server's hash and no full container resync follows

### Requirement: Click prediction
The client SHALL predict every container click type locally with vanilla semantics.

#### Scenario: Pickup, quick move, swap, throw, drag, double click
- **WHEN** each click type is performed
- **THEN** the local menu equals the server's result

### Requirement: Digging
The client SHALL compute destroy progress with vanilla rules and send start, stop and abort actions in vanilla order.

#### Scenario: Tool speed
- **WHEN** digging stone with a pickaxe
- **THEN** the break completes in the vanilla tick count

### Requirement: Block prediction
The client SHALL apply predicted block changes under a sequence id and roll them back when the server rejects them.

#### Scenario: Rejected break
- **WHEN** the server acknowledges a sequence without changing the block
- **THEN** the predicted state is reverted

### Requirement: Item use
The client SHALL try `use_item_on` and fall back to `use_item` in vanilla order, honouring cooldowns.

#### Scenario: Cooldown
- **WHEN** an item is on cooldown
- **THEN** no use packet is sent

### Requirement: Vehicles
The client SHALL mount, ride and dismount vehicles and send `player_input` and `move_vehicle`.

#### Scenario: Boat
- **WHEN** the player rides a boat
- **THEN** the boat is simulated and the server sends no corrective teleport

### Requirement: Edit actions
The client SHALL expose sign, book, rename, trade, beacon and enchant actions.

#### Scenario: Sign edit
- **WHEN** `open_sign_editor` arrives
- **THEN** the edit state is exposed and `sign_update` can be sent

### Requirement: Server state
The client SHALL store recipe book, advancements, maps, merchant offers, cooldowns, stats and waypoints, and effects and attributes SHALL feed physics.

#### Scenario: Cooldown packet
- **WHEN** `cooldown` arrives
- **THEN** the item is reported on cooldown until it expires

### Requirement: Movement without terrain
The client SHALL send no movement while the chunk at the player's position is not loaded, and SHALL let a dead player in without waiting for chunks.

#### Scenario: Join while dead
- **WHEN** a player saved dead joins and the server sends no chunks
- **THEN** the client reports loaded, stays in place and the server sends no corrective teleports

### Requirement: Bounded server state
Every collection fed by server packets SHALL be bounded in total, evicting on overflow, and server-provided numeric item data SHALL be clamped before use.

#### Scenario: Hostile packets
- **WHEN** many near-limit packets or non-finite durations arrive
- **THEN** stored state stays within its caps and no arithmetic overflows

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

### Requirement: Sleeping
While the server reports the local player sleeping, the client SHALL block movement input, use the
sleeping box and bed camera, hide the first-person hand, fade the screen and show the in-bed chat screen
whose Leave Bed button and Esc send STOP_SLEEPING.

#### Scenario: Leave bed
- **WHEN** the player lies in a bed at night and presses Leave Bed
- **THEN** a STOP_SLEEPING player command is sent and the screen closes

### Requirement: Weapon and item use parity
Left-click with a piercing weapon SHALL send the STAB action instead of an attack or a dig. While the
server reports the player using an item, releasing the use key SHALL send RELEASE_USE_ITEM even when the
client did not predict the use, and attack, dig, repeated use and pick clicks SHALL be ignored.

#### Scenario: Bundle
- **WHEN** the player holds use on a bundle and releases it
- **THEN** RELEASE_USE_ITEM is sent and the bundle is not emptied by one click

#### Scenario: Shield up
- **WHEN** the player blocks with a shield and clicks attack
- **THEN** no attack packet is sent

### Requirement: Interaction ranges and targets
Entity and block targeting SHALL use the synced `entity_interaction_range` and `block_interaction_range`
attributes and the entity's current dimensions, and an entity interaction SHALL send the hit location
relative to the entity.

#### Scenario: Survival entity reach
- **WHEN** a survival player looks at a mob 4 blocks away with default attributes
- **THEN** the mob is not targeted

#### Scenario: Armour stand
- **WHEN** the player right-clicks an armour stand's helmet with an empty hand
- **THEN** the interact packet's location has the helmet's height

### Requirement: Adventure mode predicates
In adventure mode the client SHALL allow digging when the held item's `can_break` matches the block and
placement when its `can_place_on` matches, and SHALL refuse both otherwise.

#### Scenario: can_break pickaxe
- **WHEN** an adventure player holds a pickaxe whose can_break matches stone and digs stone
- **THEN** START_DESTROY_BLOCK is sent

### Requirement: Per-tick mining
Survival digging SHALL accumulate destroy progress every tick from the current state (ground, fluid,
effects, tool) and restart when the target or held item changes, with a 5-tick delay between blocks.

#### Scenario: Jump mid-dig
- **WHEN** the player jumps halfway through digging a block
- **THEN** the remaining time grows by the off-ground factor

### Requirement: Placement state prediction
Block placement prediction SHALL use the block's placement state, including slab merging, and SHALL test
that state's collision against every entity before predicting.

#### Scenario: Stairs
- **WHEN** the player places oak stairs on the top half of a block's side
- **THEN** the predicted state is upside-down stairs facing away from the player, shown before the server
  answers

### Requirement: Adventure predicates on block entity data
`can_break` and `can_place_on` predicates SHALL match block entity NBT as vanilla's adventure check
(`BlockPredicate#matches(BlockInWorld)`) does, which leaves the component matchers out.

#### Scenario: Named chest only
- **WHEN** an adventure player holds a pickaxe that may break only chests with a given custom name
- **THEN** mining another chest is not predicted

### Requirement: Loyal trident return
A loyal trident SHALL start returning only after it dealt damage, spent more than four ticks in the ground
or lost its physics.

#### Scenario: Trident in the ground
- **WHEN** a loyal trident sticks in a block
- **THEN** it starts returning on its fifth tick in the ground
