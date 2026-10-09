## Why

Every packet needs hand-written `Encode`/`Decode`. A derive macro removes that boilerplate, and the generic protocol types vanilla uses everywhere (bit sets, block positions, identifiers, holders) belong in `mcr-buf`.

## What Changes

- New proc-macro crate `mcr-buf-derive` providing `#[derive(Encode, Decode)]` for structs and enums, re-exported from `mcr-buf` behind a default-on `derive` feature.
- Field attributes `varint`, `varlong`, `with`, `max_len`; enum attribute `tag`.
- New `mcr-buf` types: `BitSet`, `FixedBitSet<BITS>`, `LengthPrefixed<T, MAX>`, `Identifier`, `BlockPos`, `Angle`, `IdOr<T>`.

Out of scope: registry-aware holder resolution, NBT/text components, packet definitions.

## Capabilities

### New Capabilities
- `buf-derive`: derive macros for the wire traits.
- `buf-protocol-types`: generic protocol helper types.

### Modified Capabilities
