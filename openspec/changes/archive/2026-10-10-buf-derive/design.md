## Context

`Encode`/`Decode<'a>` exist in `mcr-buf`. See proposal.md.

## Decisions

- Derive output calls trait methods on field types; `varint`/`varlong` route through `VarInt`/`VarLong`, `max_len` through `encode_str`/`decode_str`. `with` calls `path::encode(&field, out)` and `path::decode(input)`.
- The Decode impl reuses the type's first lifetime as `'a`, or adds a fresh one when none exists; type parameters get `Decode<'a>` bounds.
- Enum discriminants become constants in the generated match so explicit values work with any tag type.
- `Identifier` stores the full `namespace:path` string plus colon index: one allocation, borrowed accessors.
- `IdOr<T>` has no registry knowledge; the caller maps ids.

## Risks / Trade-offs

- Derive cannot see field types, so `varint` on a non-i32 field fails at compile time through helper signatures.
