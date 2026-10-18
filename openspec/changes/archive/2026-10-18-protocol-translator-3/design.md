# Design

## Context

Translator-1 added the byte-level `Wire` seam and the 26.2 rewrites; translator-2 added sessions, extra
packets and a direct 26.1.2 translator that reuses the 26.2 rewrites with its own tables. 1.21.11 is the last
release before 26.1 (manifest order: 26.1, then 1.21.11, released 2025-12-09; protocol 774, Java 21).

## Survey: 1.21.11 against 26.1.2 and 26.3

Sources: both versions' data generator reports, and a token-level diff of the decompiled packet and codec
classes (1.21.11's client remapped with Mojang's published mappings, kept out of the repository).

- **Packets**: same sets in every state except play. Clientbound 1.21.11 lacks `game_rule_values` and
  `low_disk_space_warning` (26.1), `add_transient_block`, `post_effects`, `swing_animation` (26.3).
  Serverbound it lacks `attack`, `set_game_rule`, `spectate_entity`/`spectator_action`; 26.3 lacks `swing`.
  Changed layouts: `set_time` (day time and a tick flag became per-clock states), `interact` (an action enum
  with interact/attack/interact-at became one interact with a location; attack is its own packet),
  `client_command` (26.1 appended `REQUEST_GAMERULE_VALUES`). Chunk sections lack the fluid count 26.1 put
  after the block count. Login, configuration, light data, movement, chat, teams, login/respawn spawn info
  are 26.1.2's.
- **Registries**: 26.1 added entries (golden dandelion, sound variants, sounds, particles, components,
  slot display types). Every 1.21.11 block state, item, entity type, particle, sound and attribute has a
  26.3 entry by name. Unmapped old ids: the `bed` block entity type, `swing_animation` and `map_color`
  components, the `color` argument type (26.2's rename to `team_color`), `swing`.
- **Data components**: `container`, `bundle_contents`, `charged_projectiles`, `use_remainder` held item
  stacks (count first, optional for containers); 26.1 holds stack templates (item, count, patch;
  containers wrap each in an optional). `jukebox_playable`, `instrument`, `provides_trim_material`,
  `chicken/variant`, `zombie_nautilus/variant` and `damage_type` were `EitherHolder`s (a bool, then a holder
  or a resource key); 26.1 sends the holder. `damage_resistant`, `provides_banner_patterns` and
  `blocks_attacks.bypassed_by` were tag keys; 26.1 sends holder sets. Item particles, advancement icons
  and `item_stack` slot displays were item stacks; 26.1 sends templates. Everything else matches 26.1.2.
- **Slot displays**: 1.21.11's eight types map to 26.1's ids 0, 1, 4, 5, 6, 8, 9, 10.
- **Entity data**: four serializers inserted (cat, cow, pig and chicken sound variants), so 1.21.11
  serializer ids 22..=38 shift; the table is declared in the translator. Field indices: 26.1 added
  `AGE_LOCKED` to `AgeableMob` (index 17), shifting every later index of its 39 subclasses by one; other
  added fields (sound variants, villager data finalized, tadpole age lock) are appended at the end of
  their classes and shift nothing. 26.1.2 to 26.3 shifts nothing.
- **Tags**: 26.1 added the `minecraft:loom_dyes` and `minecraft:loom_patterns` item tags, which the
  client's loom prediction reads; a 1.21.11 server does not send them.
- **Component hashes** (click validation): 1.21.11's stack codec writes the count even when it is 1, so
  hashes of bundle contents, charged projectiles and use remainders differ from 26.1's; before 26.3 pot
  decorations hash as a list of four item ids.
- **Text components, NBT, chunk palettes, heightmaps, light, known packs, data registries**: unchanged.
  Every 1.21.11 synced registry entry exists in 26.3's data, so the known-pack claim keeps working;
  `world_clock` does not exist in 1.21.11, so the client never receives it.

## Decisions

### Direct 1.21.11 → 26.3, not a chain through 26.1.2

A chain stage 1.21.11 → 26.1.2 would itself have to walk every item stack (to turn stacks into templates)
and every chunk and entity data packet, with 1.21.11-layout readers and 26.1.2-layout writers, and then the
26.1.2 translator walks the same bytes again. The per-packet cost of every heavy packet would be the sum of
both passes, and the stage needs a non-canonical target packet set. The direct translator instead reuses
all 26.2 and 26.1.2 rules (tables absorb every id change) and adds an era check at the few places where
1.21.11 encodes a value differently, so every packet is still one pass. Measured in the bench (below): the
direct translator costs the same as the 26.1.2 one on every frame but chunks (where it adds the fluid
count 1.21.11 lacks), while a chain costs at least both passes added.

### Era in the shared rewrites

`Maps` gains `era: Era` (`Stacks` for 1.21.11, `Templates` for 26.1 and newer) and a `slot_display` remap
from the tables. The item codec branches on the era where stacks became templates (`template`, `container`),
where `EitherHolder`s and tag keys became holders and holder sets, and in both directions so creative
stacks go back in 1.21.11's shape. A resource-key `EitherHolder` cannot be turned into a registry id without
the server's registry order, so that component is left out (the client shows the item's default); vanilla
servers only produce key forms in default components, which are never sent. A canonical direct holder set
going back to a tag key field leaves the component out (`blocks_attacks` keeps the rest with no bypass).

### Entity data indices keyed by tracked spawns

`set_entity_data` carries only an entity id, so the 1.21.11 `add_entity` rule records ids whose canonical
type is one of the 39 `AgeableMob` subclasses in a session set (at most 65 536 ids; past that new ids are not
recorded and their data stays unshifted). `remove_entities` forgets ids, `login` and `respawn` clear the
set. Indices 17 and up of a recorded entity shift by one; serializer ids go through the declared table, then
the 26.2 value rewrites run unchanged.

### Interact, attack and spectating

1.21.11's client sent interact-at (location relative to the entity) and then interact unless the
client-side armor stand logic consumed the click. The translator sends both for every canonical
`interact`: only armor stands react to interact-at, and the server's own checks make the second packet a
no-op in the common cases. `attack` becomes interact with the attack action and the sneak state the session
tracks from `player_input`. `spectator_action` with an entity becomes the attack action, which a 1.21.11
server turns into spectating; stopping is the sneak key, so the empty action is dropped.

### World clock

`set_time` becomes `{game_time, [clock 0: total ticks = day time, partial 0, rate 1 or 0]}`. The client
takes its day time from the lowest clock id, and 1.21.11 has only the overworld's day time.

### Chunk fluid counts

The count is part of the 26.3 section layout and `mcr-world` keeps and re-encodes it (nothing reads it
yet), so the translated section carries the true value rather than a placeholder. The 1.21.11 `sections`
path copies the 2-byte block count, reserves 2 bytes and patches in a count computed while the block
palette is remapped: a lazily built bitset over canonical states marks states with a fluid
(waterlogged blocks included); single-valued sections count 4096 or 0, palettes without a fluid entry
skip the data scan, and palettes with one compare every lane of each packed word at once (see
Measurements). No allocation per section.

### Synthesized item tags

The 1.21.11 `update_tags` rule appends `minecraft:dyes`, `minecraft:loom_dyes` and
`minecraft:loom_patterns` to the item registry's tags when the server omits them, with vanilla 26.3's
members. A 1.21.11 loom accepts dye items and stacks with `provides_banner_patterns`, which vanilla gives
exactly the tagged items, so the loom screen predicts what the server does for vanilla items.

### Hash dialect

Click packets carry hashes of the predicted slot components, and a mismatch makes the server resend the
slot. `Wire::hash_dialect()` reports `stack_count_always` (1.21.11) and `pot_decorations_as_items`
(every translated version, since 26.3 changed that encoding), and the client's hasher follows it. This
also fixes pot decoration hashes on 26.1.x and 26.2 servers.

## Risks / Trade-offs

- Entities past the 65 536-id cap or spawned before the session started (none: spawns follow login)
  keep 1.21.11 indices.
- Armor stands receive interact after interact-at even when interact-at consumed the click.
- Resource-key `EitherHolder` values in item patches lose that component.

## Measurements

Criterion bench `translate` in `mcr-protocol`, inbound translation per frame (median), built from each
version's tables. The chunk is full height with a six-entry palette (water included) in every section, so
every 1.21.11 section takes the fluid scan; the shulker box holds 27 stacks; entity data is a sheep's.

| Frame | 26.2 | 26.1.2 | 1.21.11 |
|---|---|---|---|
| chunk (24 sections) | 3.6 µs | 3.8 µs | 9.4 µs |
| inventory (46 slots) | 2.78 µs | 2.82 µs | 2.87 µs |
| shulker box | 589 ns | 594 ns | 568 ns |
| entity data | 77 ns | 82 ns | 75 ns |
| move | 24 ns | 24 ns | 24 ns |
| keep alive | 8.4 ns | 8.3 ns | 8.1 ns |

Decoding a translated frame costs the same for all three, since it is the same 26.3 bytes. The 1.21.11
chunk's extra time is the fluid count: a per-entry scan measured 52 µs; comparing all lanes of each
packed word against the fluid palette indices at once (width-specialized, up to four fluid entries, a
per-entry fallback beyond) brings it to 9.4 µs, and sections whose palette has no fluid skip the scan. A
chain through 26.1.2 would pay the 26.1.2 pass on top of a stage that walks the same sections and
stacks: at least 13 µs per chunk and 5.7 µs per inventory.

## Live verification

Vanilla 1.21.11 (flat world, offline mode, port 25572), `MCR_LIVE_ANY_VERSION=1`, every live test file:
translate, chat, clicks, components, dead_join, dig, edits, elytra, menus, mounts, physics, live (2),
seats, use and vehicles all pass, with no untranslatable packet logged. The first run found what the
byte-level survey had missed or could not show:

- `invalid palette bits` on the first chunk: the missing fluid count (fixed above).
- `live_components`: the server corrected clicks on bundles, crossbows and use-remainder items (count
  hashed when 1), fixed by the hash dialect. It then clicks 28 component-carrying stacks without a
  correction, one more than on 26.3: 1.21.11 goat horns have no default instrument, so the `instrument`
  component travels (and translates) as a patch.
- `live_menus`: the loom predicted no dye and no pattern (missing tags), fixed by the synthesized tags.

Two reruns failed only from test order on a reused world, not from translation: `live_translate` after
`live_physics` left the server peaceful (its zombie never spawns), and `live_elytra` with a stored player
far from spawn (its platform fill hits unloaded chunks). Both pass on a reset world.

Offscreen app join (release build, `--connect --screenshot --autopilot`): joins, renders a mob scene (a
villager, cat, cow, horse, adult red and baby sheep, baby pig, all with shifted entity data, and an armor
stand in gold armor) at noon, dusk and night through the translated world clock, opens the inventory
(sword, bundle, shulker box, goat horn, potion, loaded crossbow, dye, water bucket, petals, map) and
sends chat; peak RSS 285 MB, mean frame 0.32 ms.

Canonical 26.3 regression (fresh flat world, same port, same suite): all 15 test files pass.

## Visual differences on 1.21.11 servers

Assets stay 26.3's, from the user's 26.3 client jar. Every 1.21.11 block, item, entity type, particle and
sound has a 26.3 entry by name, so nothing renders missing. Compared with a 1.21.11 client:

- 22 textures differ: crimson roots in pots, frogspawn, hay block side, pink petals (block and item),
  respawn anchor top, stripped spruce log top, map and filled map items, black llama body, horse black
  dots markings, nautilus and zombie nautilus (with coral), parched skeleton, the creative inventory tab,
  enchanting table GUI, the trial available icon and four map decorations (player, off limits, off map,
  woodland mansion).
- 26.1 moved or renamed about 124 entity textures and added dedicated baby models and textures; babies on
  a 1.21.11 server get 26.3's baby models, not 1.21.11's scaled adults. Beds and some other block entities
  are model based in 26.3.
- 1515 asset files are new in 26.x (sulfur, golden dandelion and other content a 1.21.11 server never
  sends), so they are unused.

## Remaining gaps

- 26.1's game rule traffic (`game_rule_values`, `set_game_rule`, the values request) has no 1.21.11
  counterpart. The client sends none of it today; if an in-game game rule screen arrives, it will need
  `/gamerule` command output on 1.21.11, since the translator drops the requests.
- Resource-key `EitherHolder` components are left out (vanilla never sends them in a patch).
- Armor stands get a plain interact after interact-at even when the 1.21.11 client would have stopped.
- Ageable mobs past the 65 536-id session cap keep 1.21.11 indices.
- The hash dialect covers the two differences the survey and the live component test found; any other
  1.21.11 hash difference makes the server resend that slot, which corrects the state at the cost of a
  packet.
- Leaving spectator mode's camera takes the sneak key, as on a 1.21.11 client; the stop action is dropped.
- Releases older than 1.21.11 are still unsupported.
