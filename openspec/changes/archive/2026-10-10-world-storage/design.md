## Context

Wire format verified against the 26.3 client: a section is `u16 non_empty, u16 fluid_count, blocks, biomes`; containers are `u8 bits, palette, longs` with no array length prefix (derived from bits); single-value is bits 0 plus one varint. Block strategy: 0 single, 1..=4 linear (stored as 4), 5..=8 hashmap, else global width `ceillog2(registry size)`. Biomes: 0 single, 1..=3 linear, else global.

## Goals / Non-Goals

- Goals: fast unpack and get, small memory, no panics on hostile input.
- Non-Goals: NBT, light engine, mesh data.

## Decisions

- In memory a container is single-value or `{palette, bits, longs}`; palettes are a `Vec<u16>` searched linearly on set only (at most 256 entries); reads are palette-indexed. Direct containers have no palette. Always unpacking to `[u16; 4096]` would cost 8 KiB per section.
- Decoding validates palette ids against the registry size and data entries against the palette length once, so `get` cannot fail.
- Block entity NBT is skipped with a small internal skipper and stored as raw bytes; the seam is a `RawNbt` type the NBT crate can later parse.
- Registry sizes come from `WorldConfig`; ids are asserted to fit u16.
- The non-empty count is recomputed on set; the fluid count is read and kept.

## Risks / Trade-offs

- Direct width mismatch from non-vanilla servers: the width is taken from the registry as vanilla does.
