# Design

## Context

Translator-1 put a byte-level seam (`Wire`) between the socket and the canonical 26.3 codecs. A `Translator`
holds generated id tables and per-packet rewrite functions; rewrites read the old layout and write 26.3's,
validating untouched fields with the canonical decoders. Translators were stateless `&'static` values and every
rewrite produced exactly one packet, which is why brewing bars and level event sounds could not be fixed.

## Decisions

### Session: bounded per-connection state shared by both directions

`Wire::inbound(state, id, payload, session, out)` and `outbound(...)` take a `&mut Session` next to the output
buffer. The returned `Translated<'a>` borrows only `out`, so the client can send packets (outbound, same
session) while it handles an inbound one. The session holds:

- the open brewing window id and whether its totals were sent (one window can be open at a time, so this is
  an `Option`, not a map);
- the core pack version the server offered (bounded string), echoed back in `select_known_packs`;
- a reusable buffer for extra frames.

`Wire::Canonical` never touches the session. A fresh session per connection (transfers make a new client).

### Extra packets

A rewrite can emit canonical packets after its own via `Cx::emit(name, ..)`. They are appended to `out` as
`[VarInt id][VarInt len][payload]` and returned as `Translated::Packets(Frames)`, an iterator that yields the
main packet first. Only rewrites that need it pay for it; the common result stays `Translated::Packet`.

### Direct 26.1.2 → 26.3, not a chain

The rewrites are written against canonical layouts: they validate untouched fields with the 26.3 decoders
(`skip_value`, `Particle::decode_options`, `ArgumentParser::decode_properties`, entity data). A chain stage
26.1 → 26.2 would need a full set of 26.2-layout skippers and a non-canonical target packet set, i.e. a
second copy of most of the 26.2 rewrites with different decoders. The actual 26.1 → 26.2 wire difference is
small: one renamed serverbound packet, three packets with added or reordered fields, and registry ids, which
the tables absorb. So 26.1.2 gets direct 26.1.2 → 26.3 tables and reuses every 26.2 rewrite, which read a
per-translator `Maps` instead of a global one, plus five rules of its own that later rules override by packet.
One pass per packet, the same cost as 26.2.

When a future version's layouts diverge from 26.2's in ways the shared rewrites cannot absorb with a table, the
shared rewrites gain an `Era` check or that version gets its own rule for the packet; the per-packet cost stays
one pass. A chain becomes worth it only once many versions accumulate; the seam allows it later (a translator
whose target is another translator's source).

### Registry names in the tables

`VersionTables.names` already holds old packet names. The generator now also stores old entry names for a few
small registries (`data_component_type`, `command_argument_type`, `block_entity_type`) under
`registry/<name>` keys, a few KiB. Translators use them to find old-only entries by name (26.2's
`swing_animation` and `map_color` components, whose ids differ in 26.1) and to apply declared renames
(26.1's `color` argument is 26.2's `team_color`) by patching the loaded map once at startup.

### Walking item values in both directions

The component walker is parametrised by direction. Forward reads old layout and writes canonical; back reads
canonical and writes old. Each changed component has one match arm with both directions next to each other:
registry ids go through `forward`/`back`, `instrument` adds or drops the durability, trim materials convert
between the inline asset suffix and the palette id, pot decorations between four optional templates and four
item ids (brick = empty), teleport consume effects add or drop the particle flag, and the split
`attack_animation`/`interact_animation` merge back into `swing_animation` (interact is dropped when attack is
present). Creative stacks are the only back-walked values: their top-level values are length-prefixed, the
nested templates are not.

### NBT the translator touches

Pot sherds: 26.2 sends `sherds` as a list of up to four item names (back, left, right, front; brick = blank).
The chunk and `block_entity_data` rewrites parse only decorated pot tags (type checked first, others copied
by the raw skipper) and rewrite `sherds` into `{back: {id, count: 1}, ...}`, leaving blanks out.

Potion predicates: 26.2's `potion_contents` component predicate is a bare holder set (string or list); 26.3's
is `{potions: <holder set>}`. Block predicates wrap it on the way in.

### Recursion and loop bounds

Every path that can recurse passes a `Depth` and calls `deeper()` before recursing: item patches, component
values holding stacks, block predicates, slot displays, and now particles (an item particle's template takes
the caller's depth instead of a fresh one). Text components and NBT that are only copied go through
`mcr_nbt::raw` (`RawValue::size`, depth-limited, no allocation) instead of `Component`/`NetworkNbt` decoding.
Canonical decoders the translator still calls have their own guards (component `DepthGuard`, slot display
guard, NBT accounter). Every loop count comes from `read_len` bounded by the remaining input or a fixed cap,
and every iteration consumes at least one byte.

Audit findings (every recursive path and packet-driven loop in `version/`):

| Path | Finding | Fix |
|---|---|---|
| `block_predicate` | recursed into item templates without `deeper()` | fixed on main (c19e314) |
| item particles (`set_entity_data`, `level_particles`, explosions) | the item inside a particle restarted at `Depth::top()`, so a particle reached from inside a stack would reset the budget | `particle` takes the caller's `Depth` |
| text components (chat, titles, teams, trims, instruments, recipe book, advancements, merchant offers) | decoded into `Component` trees only to be re-encoded: depth bounded by the component guard, but one allocation per node | copied by `io::text` through the raw NBT skipper |
| NBT (block entities, entity data, `custom_data`, exact matchers) | decoded into `NetworkNbt` to be copied, same allocation pattern | copied by `io::nbt`/`io::compound`, depth 512, no allocation |
| item patches, slot displays | already depth-limited; binary slot displays fan out at most 2^16 nodes, each consuming input | hostile tests added |
| loops (patch counts, holder sets, recipe and advancement lists, chunk block entities, bit sets, palettes) | all bounded by remaining input or a cap, each iteration consumes at least one byte; pre-allocations capped (`min(64/256/4096)`) | hostile tests for `i32::MAX` counts on tiny payloads |
| new session state | one brewing window, one core pack version (64 chars max) | bounded by construction |

Tests: `nested_particle_items_hit_the_depth_limit`, `deep_text_and_nbt_fail_cleanly` (100k nested lists),
`nested_slot_displays_hit_the_depth_limit`, `huge_counts_with_tiny_payloads_fail_fast`,
`nbt_copies_without_decoding_and_bounds_depth`, and per-rule random payload fuzzing for both translators.

### Level event sounds

26.3 moved the sounds out of level events 2002/2007 (splash particles) and 3003 (wax on) into separate
packets: 1053/1054 level events for splashes and a `sound` packet for waxing. 26.2 servers send only the
particle event, so the rewrite emits the 26.3 sound packet after it at the same position.

## Risks / Trade-offs

- The 26.1.x family shares protocol 775; the translator is built from 26.1.2's reports. 26.1 and 26.1.1 use
  the same protocol and registries, and all three passed live tests, so all three are listed as supported.
- Brewing totals are 26.2's constants (400 ticks, 20 fuel); 26.2 has no other values.
- Back-walked trims whose palette is not `minecraft:trim/<name>` lose their material on the way back.

## Measurements

`cargo bench -p mcr-protocol --bench translate` (criterion, synthetic frames built from each version's tables;
inbound translation, then canonical decode of the result for scale):

| Frame | 26.2 → 26.3 | 26.1.2 → 26.3 | Canonical decode of the result |
|---|---|---|---|
| chunk (24 sections, 4 chests, full light, ~97 KB) | 3.34 µs (28 GiB/s) | 3.42 µs | 0.15 µs (borrowed) |
| 46-slot inventory, 15 stacks with components | 2.98 µs | 2.95 µs | 4.36 µs |
| `move_entity_pos` | 24 ns | 24 ns | 165 ns |
| `keep_alive` | 8 ns | 9 ns | 152 ns |

The direct 26.1.2 translator costs the same as 26.2's: one pass per packet. `--bench dispatch` (whole play
stream): 2.171 ms decoding directly, 2.170 ms through `Wire::Canonical` with a session, so the canonical wire
stays free.

Live (vanilla servers on loopback, offline mode): the full mcr-client live suite (15 test files) passes against
26.1.2 and 26.2 with `MCR_LIVE_ANY_VERSION=1` and against 26.3 canonically; 26.1 and 26.1.1 pass the subset
translate, chat, components, menus, clicks and use. The offscreen app joins every version, renders a decorated
pot with a translated sherd and opens a brewing stand with fuel and brew progress bars. `live_vehicles` and
`live_mounts` assume a superflat world (they teleport to y = -60), so they fail on normal terrain regardless of
version.

## Remaining gaps

- Hashed stacks in container clicks: components whose value changed between versions (split swing animations,
  trims, instruments, pot decorations, consume effects, attribute modifiers) keep the client's 26.3 hash, so the
  old server resends the slot after such a click. That costs one extra packet and does not desync anything.
- 26.3-only content has no old id: creative stacks of 26.3-only items are not sent, and hashed stacks holding
  them are sent as empty (the server resends the slot).
- Brewing totals are 26.2's fixed constants (400 brew ticks, 20 fuel). No 26.2 or 26.1 server can send other
  values.
- Trims walked back whose material palette is not `minecraft:trim/<name>` lose their material.
- 26.1 bed block entities are dropped. 26.3 renders beds from block state, so nothing visible is lost.
- Versions before 26.1 (protocol < 775) are not translated.
