## Why

Inventory, container, creative and many entity packets carry item stacks. A stack embeds data components with no length prefix, so one unmodelled component makes the rest of a packet unreadable. The `ItemStack` and `HashedStack` placeholders must become real codecs.

## What Changes

- `ItemStack` (optional stack: count 0 means empty), plus `NonEmptyItemStack`, `UntrustedItemStack` (per-component length prefix, used by creative-slot) and `ItemStackTemplate` wrappers.
- `DataComponentPatch` and a `DataComponent` enum covering all 122 component types of 26.3, ids matching the `data_component_type` registry.
- Sub-types: holders (`IdOr`), `HolderSet`, consume effects, mob effect instances, block predicates with component matchers, profiles, enchantments, attribute modifiers, books, fireworks, and so on.
- `HashedStack` with its hashed patch map (encoding only).

Out of scope: computing component hashes (CRC32C of the hashed representation), registry-aware validation, tooltips, and the text component, particle and sound codecs, which other changes own.

## Capabilities

### New Capabilities
- `protocol-items`: wire codecs for item stacks, data components, patches and hashed stacks.

### Modified Capabilities

None.

## Impact

`crates/mcr-protocol/src/types/item.rs` and `types/component.rs` (with submodules). No new dependencies. Tests that reach text components or sound events are ignored until those codecs land.
