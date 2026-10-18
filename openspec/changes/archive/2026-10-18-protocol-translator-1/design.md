# Design

## Context

Packets are hand-written canonical types (`mcr_protocol::{handshake,status,login,configuration,play}`) grouped
into one dispatch enum per state and direction (`dispatch.rs`). Ids come from `PacketIds`, resolved by name from
the `mcr-data` blob that `mcr-data-gen` builds out of 26.3's data generator reports. `mcr-net` frames bytes on
blocking I/O threads and hands `(id, payload)` to the game thread, where `mcr-client` decodes per phase. Every
registry id the client interprets (block states in chunks, items, entity types, sounds, particles, data
components) is a 26.3 id read through `mcr_data::v26_3()`.

## Goals / Non-Goals

Goals: one translation seam for every state and direction; nothing version-specific outside
`mcr_protocol::version`; canonical traffic pays nothing; 26.2 playable against a real server.
Non-goals: chained translation over many versions (the seam allows it later), older assets, mod mode.

## Decisions

### Frame-level seam on the game thread

`Wire` is a `Copy` enum: `Canonical` or `Legacy(&'static Translator)`. The client calls
`wire.inbound(state, id, payload, scratch)` before decoding and `wire.outbound(state, id, payload, scratch)`
after encoding. Translation works on bytes, not on typed packets, because most packets only need an id change and
the rest only need a few fields touched; copying the untouched tail is cheaper than decoding and re-encoding.

The game thread owns the protocol phase, so translation runs there, in the order frames are handled. The reader
thread's phase is racy around state switches (the server may send the next state's first packet before the game
thread has switched), which is harmless for the compression hook but not for translation.

`Canonical` returns `Same(id)` without touching the payload: one predictable branch per frame. The criterion bench
`dispatch/play stream` gains a `decode via canonical wire` case next to plain decode.

Alternatives: per-packet typed conversion (old struct → canonical struct) needs an old type for every changed
packet and costs a full decode/encode; a ViaVersion-style pipeline object per connection allocates per packet.

### Translator shape

A translator holds the generated tables plus, per state and direction, an array indexed by wire id giving the
canonical id and an optional rewrite function (`fn(&Translator, &mut Reader, &mut Vec<u8>) -> Result<Action>`).
Outbound has the same array indexed by canonical id. Rewrite helpers copy primitives, remap VarInt registry ids,
and walk the shared codecs (item stacks with their data components, particles, entity data, chunk sections, sound
holders, slot displays) with old and new layouts. Every count read from the payload is checked against the same
caps the canonical decoders use, and nesting (item stacks inside components) is depth-limited.

Actions: `Rewritten(id)` (payload in scratch), `Same(id)`, `Drop`. Packets present in only one version get a
synthesizer (e.g. 26.2 `animate` swing → 26.3 `swing_animation`; 26.3 `punch` → 26.2 `swing`) or `Drop`.

### Generated tables, committed

`mcr-data-gen` gains `diff` (`version_tables(old_reports, new_reports) -> Vec<u8>`), called by
`cargo xtask version-tables`. Output is `crates/mcr-protocol/src/version/v26_2/tables.bin`: little-endian,
`MCRV` magic, both protocol numbers, then named `u16` arrays (`0xFFFF` = absent) for each packet set (old→new and
new→old) and for each registry whose ids differ (identical registries are omitted and read as identity). Block
states are matched by block name plus property values. The file is a set of id facts derived from reports, the
same category as `mcr-data`'s committed blob, and a test regenerates and compares it when `.cache` has both
versions' reports.

### Protocol choice

`ClientConfig::wire` is `WireChoice::{Canonical, Protocol(i32), Auto}`. Auto pings status on its own short
connection (5 s, 32 KiB response cap, JSON parsed only for `version.protocol` and `version.name`). The app passes
the server list's pinged protocol, auto for direct connects and transfers, and canonical for its local server.
Mod mode refuses a non-canonical wire before login: Java's client is 26.3 and the mirror forwards canonical
frames.

### Launcher

`SUPPORTED_VERSIONS` gains `26.2` only after the live test. A plan for any supported version loads assets from
the canonical (26.3) jar; the version a profile names only matters as the oldest protocol the user expects.

## Risks / Trade-offs

- Item components and entity data differ in small ways that are easy to miss; a missed difference shows up as a
  dropped packet (logged once per packet type), not a crash. The live checklist covers the common paths.
- Hashed stacks in `container_click` are hashed from canonical component values; a component whose value changed
  between versions hashes differently and the server just resends the slot (vanilla behaviour on mismatch).
- 26.3-only content (new blocks, items, entities) never arrives from a 26.2 server; outbound references to it fall
  back as specified.

## Visual limits on older servers

The client renders with 26.3 assets and data. Blocks, items and entities the old server sends look like their
26.3 versions (retextures and model changes in 26.3 apply). Old-only behaviour driven by client-side data (e.g.
26.2 default components that 26.3 changed) follows 26.3. Older resource packs from a 26.2 server are offered
as-is and may not load where formats changed.

## Known gaps in the 26.2 translator

Each of these drops or approximates data; none blocks play.

- Creative-mode stacks going back to the server keep only components whose value encoding did not change
  between versions (others are omitted; the server fills defaults).
- Level events 2002/2007 (splash potions) and 3003 keep their data as-is; 26.3-only sounds they imply are not
  synthesized.
- Decorated pot sherds inside block entity NBT and `potion_contents` predicates in NBT keep their 26.2 shape.
- 26.3-only synced registries (`decorated_pot_pattern`, block transformers, block state providers) come from the
  client's own 26.3 data through the known-pack claim.
- Entity events added in 26.3 never arrive. Brewing stand slot rules are synthesized as the 26.3 property
  sets, but 26.2 sends only two brewing data values, so the fuel and progress bars stay empty.

## Measurements

The `dispatch` criterion bench over the 886 captured 26.3 play frames: direct decode 2.134 ms, decode through
`Wire::Canonical` (kept opaque with `black_box`) 2.149 ms, about 17 ns per frame.

## Live verification

Against a vanilla 26.2 server, every `mcr-client` live test (`live_translate` plus the existing dig, clicks,
components, menus, edits, use, physics, dead join, mounts, vehicles, seats and elytra tests) passes with
`MCR_LIVE_ANY_VERSION=1`. The offscreen app joined, chatted, dug, opened the inventory, saw summoned entities and
showed advancement and recipe toasts.
