# Proposal

## Why

Translator-1 made 26.2 playable but left known gaps (empty brewing bars, creative stacks losing components,
missing splash and wax sounds, pot sherds and potion predicates in their 26.2 shape), and 26.1.x servers
(protocol 775, still common) cannot be joined at all. A security review also found an unbounded recursion in
the translator, so every recursive path needs the same audit.

## What Changes

- **Recursion audit.** Every recursive or mutually recursive path in the translator goes through
  `Depth::deeper()`, and every loop is bounded by remaining input or a cap. Particles carry the caller's depth
  instead of restarting at the top. Text components and NBT the translator only copies are validated with the
  allocation-free raw NBT skipper instead of being decoded into trees. Hostile-input tests for each path.
- **Per-connection translator session.** `Wire::inbound`/`outbound` take a `wire::Session` holding bounded
  per-connection state shared by both directions, and a rewrite may emit extra canonical packets after the
  one it translates (`Translated::Packets`).
- **26.2 gaps closed:**
  - brewing stands get 26.3's total brew time (400) and total fuel (20) data values when the brewing window
    opens, so both bars draw;
  - creative-mode stacks going back keep every component: values whose encoding changed are walked back to
    26.2's layout (registry ids, sounds, holder sets, block predicates, trims, pot decorations, instruments,
    consume effects, the split swing animation);
  - level events 2002/2007 are followed by 26.3's 1053/1054 splash sound events, and 3003 by the wax-on sound;
  - decorated pot `sherds` in block entity NBT become 26.3's side compound, and `potion_contents` component
    predicates become 26.3's `{potions: ...}` shape;
  - the known core pack claim echoes the version the server offered instead of a hard-coded name.
- **26.1.x (protocol 775)** through a direct 26.1.2 → 26.3 translator that reuses the 26.2 rewrites with its
  own tables, plus its own rules for the five packets that differ (`login`, `login_finished`,
  `set_player_team`, `spectator_action`, attribute and block entity type ids). Added to the supported versions
  only after the live suite passes.
- **Tables** gain old-side names for a few small registries (data component types, command argument types,
  block entity types) so translators resolve renamed or removed entries by name; registry renames are declared
  per translator.
- **Benchmarks** for the translated paths (chunks, item stacks) per version.

## Out of scope

Versions older than 26.1, the reverse direction, mod mode on older servers, 26.1-era assets (26.3 assets
always), block entity NBT changes the client does not read.

## Capabilities

### Modified Capabilities
- `protocol-translation`: session state and extra packets, recursion bounds, 26.2 gaps, 26.1.x translator,
  version lookup for 775.
- `launcher`: the unsupported-version example moves to an older release.
- `dev-tooling`: version tables carry old registry names for named registries.

## Impact

`mcr-protocol` (version layer, tables, benches), `mcr-data-gen` (registry names), `mcr-client` (session in
the connection, multi-packet results), `mcr-launcher` tests.
