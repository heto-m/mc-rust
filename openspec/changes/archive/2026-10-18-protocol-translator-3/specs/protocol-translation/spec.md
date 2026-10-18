## MODIFIED Requirements

### Requirement: Version lookup
`mcr_protocol::version` SHALL expose the supported versions newest first with their protocol numbers, and `Wire::for_protocol(n)` SHALL return the canonical wire for 777, the 26.2 translator for 776, the 26.1.2 translator for 775 (which serves 26.1, 26.1.1 and 26.1.2), the 1.21.11 translator for 774, and nothing for any other number.

#### Scenario: Unknown protocol
- **WHEN** a server reports protocol 770
- **THEN** `Wire::for_protocol` returns nothing and the join is refused with the server's version name

#### Scenario: 26.1.x server
- **WHEN** a server reports protocol 775
- **THEN** `Wire::for_protocol` returns the 26.1.2 translator

#### Scenario: 1.21.11 server
- **WHEN** a server reports protocol 774
- **THEN** `Wire::for_protocol` returns the 1.21.11 translator

### Requirement: Translator session
Each connection SHALL own one translator session passed to every inbound and outbound translation. It SHALL hold only bounded state: the open brewing window, the core pack version the server offered, the ids of spawned entities whose data indices need shifting (at most 65 536), the sneak key state, and reusable buffers. The canonical wire SHALL ignore it.

#### Scenario: Known pack echo
- **WHEN** a 26.1 server offers `minecraft:core` version `26.1` and the client answers with its 26.3 core pack
- **THEN** the server receives `minecraft:core` version `26.1`

#### Scenario: Entity id cap
- **WHEN** a 1.21.11 server spawns more ageable mobs than the cap without removing any
- **THEN** the session stops recording new ids and its size stays at the cap

## ADDED Requirements

### Requirement: 1.21.11 translator
The 1.21.11 translator SHALL translate protocol 774 directly onto 26.3 in one pass per packet, with its own generated tables and every 26.2 and 26.1.2 rewrite, and SHALL turn `set_time` into a world clock update for clock 0.

#### Scenario: World time
- **WHEN** a 1.21.11 server sends `set_time` with day time 6000 and a ticking day cycle
- **THEN** the client receives clock 0 at 6000 ticks with rate 1

### Requirement: Item values from 1.21.11
The 1.21.11 translator SHALL read item values in 1.21.11's shape where 26.1 changed them: item stacks where 26.1 has stack templates (container, bundle and charged projectile contents, use remainders, item particles, advancement icons, item stack slot displays), `either` holders where 26.1 has holders, tag keys where 26.1 has holder sets, and slot display type ids. Creative stacks SHALL go back in the same 1.21.11 shapes. A resource-key `either` holder SHALL leave its component out.

#### Scenario: Shulker box contents
- **WHEN** a 1.21.11 server sends a shulker box whose `container` component holds a stack of 3 diamonds in slot 0
- **THEN** the canonical component holds an optional template of 3 diamonds in slot 0

#### Scenario: Key-form holder
- **WHEN** a 1.21.11 stack carries `instrument` as a resource key
- **THEN** that component is left out and the rest of the stack translates

### Requirement: Interactions sent to 1.21.11
A canonical `interact` SHALL be sent to a 1.21.11 server as interact-at followed by interact; `attack`, and `spectator_action` with an entity, SHALL be sent as interact with the attack action and the sneak state the session tracks; game rule requests and edits SHALL be dropped.

#### Scenario: Attacking a mob
- **WHEN** the client attacks entity 42 while sneaking on a 1.21.11 server
- **THEN** the server receives `interact` for entity 42 with the attack action and the secondary action set

### Requirement: Entity data indices on older servers
When an older version defines fewer synched data fields in a superclass than 26.3, the translator SHALL shift the later indices of every affected entity type to 26.3's, keyed by entity ids recorded from spawns, and SHALL remap entity data serializer ids by table. On 1.21.11 every index from 17 up of an `AgeableMob` subclass SHALL shift by one.

#### Scenario: Sheep wool
- **WHEN** a 1.21.11 server sends a sheep's wool byte at index 17
- **THEN** the canonical entity data carries it at index 18

#### Scenario: Not an ageable mob
- **WHEN** a 1.21.11 server sends index 17 for a zombie
- **THEN** the index stays 17

### Requirement: Chunk sections from 1.21.11
The 1.21.11 translator SHALL add the fluid count 26.1 put after each section's block count, computed from the section's translated block states (states with a fluid, including waterlogged blocks), without allocating per section.

#### Scenario: Water section
- **WHEN** a 1.21.11 server sends a section filled with water
- **THEN** the canonical section header carries a fluid count of 4096

### Requirement: Item tags older servers lack
When a 1.21.11 server's `minecraft:item` tags lack a tag 26.3's client logic reads (`dyes`, `loom_dyes`, `loom_patterns`), the 1.21.11 translator SHALL append it with vanilla 26.3's members so screens predict the server's own behavior.

#### Scenario: Loom on 1.21.11
- **WHEN** a 1.21.11 server sends its item tags
- **THEN** the client sees `minecraft:loom_dyes` holding the 16 dyes and `minecraft:loom_patterns` holding the banner pattern items

### Requirement: Hash dialect
Each wire SHALL report how the server hashes item components for click validation where that differs from 26.3, and the client SHALL hash predicted stacks in that dialect: 1.21.11 hashes an item stack's count even when it is 1, and servers before 26.3 hash pot decorations as a list of four item ids with bricks for empty sides.

#### Scenario: Bundle with one item on 1.21.11
- **WHEN** the client predicts a click on a bundle holding one stick on a 1.21.11 server
- **THEN** the predicted hash includes the stick's count and the server accepts the click without correcting it
