## Context

Vanilla 26.3 writes components through each type's `STREAM_CODEC`; the type id is a registry VarInt. Components without a hand-written stream codec (custom_data, lock, recipes, ...) fall back to network NBT.

## Decisions

- `DataComponent` is one enum variant per registry entry, declared once in a `components!` macro table (id, name, variant, payload type) that also generates encode, decode, `type_id` and `name`. A test checks the table against `registries.json` when the data cache exists.
- Registry-backed holders (items, attributes, enchantments, variants) are plain VarInt ids; they are server-synced so the client resolves them elsewhere. Holders with inline data use `IdOr<T>`.
- NBT-fallback components keep the raw `NetworkNbt`; component predicates in `DataComponentMatchers` are NBT too, kept verbatim for byte-exact round trips.
- Maps (enchantments, block state, hashed patch) are `Vec` of pairs in wire order, so re-encoding is byte-identical.
- Recursion (containers, bundles, predicates, mob effects) is bounded by a thread-local depth guard of 32 on decode; list lengths are bounded by remaining input or the vanilla maximum.
- Untrusted stacks wrap each component value in a VarInt length prefix; decoding hands each value its own slice.

## Risks

Component payload layouts are transcribed from decompiled 26.3; mistakes surface only against a real server, so fixtures cover the simple cases byte-exactly and the rest round-trip.
