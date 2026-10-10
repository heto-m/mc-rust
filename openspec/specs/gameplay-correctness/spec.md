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
