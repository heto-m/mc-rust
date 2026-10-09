## Context

Packets are plain structs deriving `Encode`/`Decode` from `mcr-buf`. Irregular codecs are hand-written.

## Decisions

- Movement packets are `Copy` structs with a `MovementFlags` byte wrapper; no allocation on encode beyond the output buffer.
- Enums sent as VarInt ordinals use the derive default tag; byte-sized ones use `#[mcr(tag = u8)]` where vanilla writes a byte.
- Blocks, hit results and `Vec3i` offsets follow the vanilla codec of each field exactly (structure block offsets and sizes are single bytes).
- The low-precision vector codec (`LpVec3`) is private to this module because only `interact` uses it here.
- `container_click` stores changed slots as an ordered list of `(slot, HashedStack)` bounded at 128, matching the map wire format.
- Bounded byte arrays (public key 512, key signature 4096) use a private `Bytes<MAX>` type.
- Registry holders (mob effects, game rules via identifier) are raw ids; resolving them is a caller concern.
- Tests with `Component`, `ItemStack` or `HashedStack` fields are ignored until those codecs land.

## Risks

- `Rotation` for `set_structure_block` and test instance packets uses the index order of vanilla (none, 90, 180, -90).
