# protocol-translation Specification

## Purpose
Lets the client join servers running older supported releases by translating their packets, packet ids and registry ids onto the canonical 26.3 packet set inside `mcr-protocol`'s version layer.

## Requirements

### Requirement: Wire between socket and canonical codecs
`mcr_protocol::version::Wire` SHALL translate every frame between the wire and the canonical 26.3 packet set, in handshake, status, login, configuration and play, for both directions. Inbound it SHALL turn a wire `(id, payload)` into canonical packets or a drop; outbound it SHALL turn a canonical `(id, payload)` into wire packets or a drop. Callers SHALL decode and encode only canonical packets. The handshake's protocol number SHALL be rewritten to the wire version's.

#### Scenario: Old server login
- **WHEN** the client joins a 26.2 server through `Wire::Legacy`
- **THEN** the handshake carries protocol 776 and every packet the client decodes is a canonical 26.3 packet

### Requirement: Extra packets from one translation
A translation MAY yield more than one packet; when it does, the translated packet SHALL come first and the extra packets SHALL follow in order.

#### Scenario: Extra packet
- **WHEN** a 26.2 server sends level event 2002 (splash particles)
- **THEN** the client receives level event 2002 followed by level event 1053 at the same position

### Requirement: Zero-cost canonical path
`Wire::Canonical` SHALL return the inbound frame unchanged without copying or table lookups, and SHALL leave outbound frames untouched. A criterion bench SHALL compare decoding the captured 26.3 play stream through the canonical wire against decoding it directly.

#### Scenario: Canonical frame
- **WHEN** a 26.3 frame passes `Wire::Canonical`
- **THEN** the same id and the same payload slice come back

### Requirement: Translator contract
A translator SHALL remap packet ids per state and direction; rewrite payloads whose fields were added, removed, reordered or re-encoded; remap registry ids in payloads; and give every packet that exists in only one version a defined fallback: synthesize the closest packet of the other version, or drop it.

#### Scenario: Shifted packet id
- **WHEN** a 26.2 server sends `set_time` under its 26.2 id
- **THEN** the client decodes it under 26.3's `set_time` id

#### Scenario: Packet only in the canonical version
- **WHEN** the client sends `punch` to a 26.2 server
- **THEN** the translator sends 26.2's `swing` with the same hand instead

### Requirement: Bounded translation
A payload that fails to parse or would exceed its bounds SHALL produce a translation error naming the packet, never a panic, unbounded loop, unbounded recursion or unbounded allocation. Inbound play errors SHALL drop the packet and be logged once per packet type; errors in login and configuration SHALL fail the connection.

#### Scenario: Truncated payload
- **WHEN** a 26.2 `level_chunk_with_light` payload is cut in half
- **THEN** translation returns an error naming the packet and nothing panics

### Requirement: Recursion and loop bounds in rewrites
Every recursive or mutually recursive rewrite path (item patches, component values holding stacks, block predicates, slot displays, particles) SHALL go through the translator's depth guard, every loop count SHALL be bounded by the remaining input or a fixed cap, and NBT or text components that are only copied SHALL be validated without building trees.

#### Scenario: Deeply nested item particle
- **WHEN** an entity data particle carries an item whose container component nests stacks past the depth limit
- **THEN** translation returns an error and the stack depth stays bounded

### Requirement: Registry remapping
Registry ids SHALL be remapped through tables matched by name: block states by block and property values, and items, entity types, sound events, particle types, data component types, custom stats, command argument types, attributes and block entity types by registry entry. An old id with no canonical entry SHALL fall back to the registry's default (air, the empty stack, or dropping the value); a canonical id with no old entry going outbound SHALL fall back the same way.

#### Scenario: Shifted block state
- **WHEN** a 26.2 chunk section carries the 26.2 id of `minecraft:stone`
- **THEN** the canonical chunk holds the 26.3 id of `minecraft:stone`

#### Scenario: New item sent to an old server
- **WHEN** the client sends a creative slot with an item that does not exist in 26.2
- **THEN** the packet is dropped

#### Scenario: Removed block entity type
- **WHEN** a 26.1.2 chunk carries a bed block entity, a type 26.3 does not have
- **THEN** that block entity is left out and the rest of the chunk translates

### Requirement: Renamed registry entries
A translator MAY declare renamed registry entries, which SHALL map as the same entry.

#### Scenario: Renamed argument type
- **WHEN** a 26.1.2 server's command tree uses the `minecraft:color` argument
- **THEN** the canonical tree uses `minecraft:team_color`

### Requirement: Generated version tables
The packet and registry id tables of each older version SHALL be generated by `cargo xtask version-tables --version <old>` from the old and canonical data generator reports in `.cache/versions`, into a binary file under `crates/mcr-protocol`, deterministically. A test SHALL regenerate them and compare when both reports are present, and skip otherwise.

#### Scenario: Reports present
- **WHEN** `.cache/versions/26.2` and `.cache/versions/26.3` hold their reports
- **THEN** regenerating the 26.2 tables yields the committed file byte for byte

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

### Requirement: Canonical assets on older servers
On an older server the client SHALL keep loading 26.3 assets and data. Content the old server cannot send SHALL simply never appear, and content that changed between versions SHALL be shown with its 26.3 look.

#### Scenario: Join 26.2 with the 26.3 jar
- **WHEN** the client joins a 26.2 server
- **THEN** textures and models come from the 26.3 client jar

### Requirement: Translator session
Each connection SHALL own one translator session passed to every inbound and outbound translation. It SHALL hold only bounded state: the open brewing window, the core pack version the server offered, the ids of spawned entities whose data indices need shifting (at most 65 536), the sneak key state, and reusable buffers. The canonical wire SHALL ignore it.

#### Scenario: Known pack echo
- **WHEN** a 26.1 server offers `minecraft:core` version `26.1` and the client answers with its 26.3 core pack
- **THEN** the server receives `minecraft:core` version `26.1`

#### Scenario: Entity id cap
- **WHEN** a 1.21.11 server spawns more ageable mobs than the cap without removing any
- **THEN** the session stops recording new ids and its size stays at the cap

### Requirement: Brewing stand data on older servers
When an older server opens a brewing stand, whose menu has two data values, the translator SHALL add 26.3's total brewing ticks (400) and total fuel (20) data values once for that window, so the progress and fuel bars draw.

#### Scenario: Brewing window
- **WHEN** a 26.2 server opens a brewing stand and sends its fuel value
- **THEN** the client also receives data values 2 = 400 and 3 = 20 for that window

### Requirement: Creative stacks translated back
Creative-mode stacks sent to an older server SHALL keep every component that exists in the old version, with values whose encoding changed (registry ids, sound holders, holder sets, block predicates, trim materials, pot decorations, instruments, consume effects, the swing animation split) rewritten to the old layout.

#### Scenario: Trimmed armour from the creative inventory
- **WHEN** the client puts a trimmed helmet with a custom name in a creative slot on a 26.2 server
- **THEN** the server receives the helmet with its 26.2 trim and name components

### Requirement: Level event sounds on older servers
Level events whose sound moved to a separate packet in 26.3 SHALL be followed by that packet: 2002 by level event 1053, 2007 by level event 1054, and 3003 by the `item.honeycomb.wax_on` block sound at the event's block centre.

#### Scenario: Waxing a copper block
- **WHEN** a 26.2 server sends level event 3003
- **THEN** the client receives level event 3003 and a `sound` packet for `minecraft:item.honeycomb.wax_on`

### Requirement: Block entity and predicate NBT from older servers
Decorated pot `sherds` in block entity NBT from servers older than 26.3 SHALL be rewritten from a list of item names to 26.3's compound of optional sides (blank sides left out), in chunks and in `block_entity_data`. `potion_contents` component predicates SHALL be rewritten from a bare holder set to `{potions: <holder set>}`.

#### Scenario: Pot with one sherd
- **WHEN** a 26.2 chunk carries a decorated pot with `sherds: ["minecraft:brick", "minecraft:brick", "minecraft:brick", "minecraft:angler_pottery_sherd"]`
- **THEN** the canonical tag has `sherds: {front: {id: "minecraft:angler_pottery_sherd", count: 1}}`

### Requirement: 26.1.x translator
The 26.1.2 translator SHALL translate protocol 775 directly onto 26.3 with its own generated tables, reusing the 26.2 rewrites, and SHALL additionally insert `online_mode` (true, 26.1's behaviour) into `login`, a nil session id into `login_finished`, reorder `set_player_team` parameters and turn its formatting color into 26.3's optional team color, send `spectator_action` with an entity as `spectate_entity` (dropping it without one), and remap attribute and block entity type ids.

#### Scenario: Team colour
- **WHEN** a 26.1.2 server creates a team with color `red`
- **THEN** the canonical `set_player_team` carries team color red after the prefix and suffix

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
