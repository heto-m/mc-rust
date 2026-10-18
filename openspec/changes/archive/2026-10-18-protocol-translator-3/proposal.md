# Proposal

## Why

Most players on older servers sit on the last release before the version renumbering: 1.21.11 (protocol 774),
released 2025-12-09, the entry right before 26.1 in Mojang's version manifest. mcr reaches 26.1.x through
26.3 but refuses 1.21.11.

## What Changes

- **1.21.11 (protocol 774) translator**, direct onto 26.3: its own generated tables, every 26.2 and 26.1.2
  rewrite, and rules for what 26.1 changed:
  - item stacks inside containers, bundles, charged projectiles, use remainders, item particles,
    advancement icons and slot displays become 26.1's stack templates; `either` holders (jukebox songs,
    instruments, trim materials, chicken and zombie nautilus variants, damage types) and tag keys (damage
    resistance, banner patterns, blocks-attacks bypass) take 26.1's encodings; slot display type ids shift;
  - entity data serializers shift by four, and entity data indices of `AgeableMob` subclasses shift by one
    (26.1 added `AGE_LOCKED`), keyed by entity ids the session tracks from spawns;
  - `set_time` becomes a world clock update;
  - `interact` splits back into 1.21.11's interact-at and interact actions, `attack` and spectating an
    entity become the attack action; game rule requests and edits are dropped.
- **Shared rewrites gain an era**: the item codec and slot displays read stacks and holders in the old
  shape when the translator says so. 26.2 and 26.1.x behaviour is unchanged.
- **Session** also tracks ageable entity ids (bounded) and the sneak key.
- **Benchmarks** for 1.21.11 next to 26.2 and 26.1.2, and the chain alternative's cost.
- 1.21.11 joins `SUPPORTED_VERSIONS` only after the live suite passes against a vanilla 1.21.11 server.
- Visual differences documented: assets stay 26.3.

## Out of scope

1.21.10 and older, mod mode on older servers, 1.21.11 assets, the reverse direction, armor stand
name-tag edge cases beyond what the interact split gives.

## Capabilities

### Modified Capabilities
- `protocol-translation`: 1.21.11 translator, version lookup for 774, session state, entity data indices.
- `launcher`: the unsupported-version example moves to 1.21.10.

## Impact

`mcr-protocol` (version layer, new `v1_21_11` tables and rules, era in the shared codec, benches),
`mcr-launcher` spec example. No client changes beyond the supported list.
