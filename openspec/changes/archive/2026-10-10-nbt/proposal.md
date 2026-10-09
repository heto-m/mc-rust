## Why

Chunk data, registries, item components and chat text all arrive as NBT. The client needs a fast, hostile-input-safe NBT library before protocol, world and asset crates can consume them.

## What Changes

- New crate `mcr-nbt` with an owned `Value` model (insertion-ordered compounds) and a borrowed zero-copy reader.
- Java Modified UTF-8 codec for names and strings.
- Disk NBT (named root, optional gzip/zlib) and network NBT (nameless root, End tag = absent) codecs, with `Encode`/`Decode` via `mcr-buf`.
- Vanilla-style limits: depth 512 and a byte quota.
- SNBT stringifier and parser.

## Capabilities

### New Capabilities
- `nbt`: reading, writing, limiting and textual representation of NBT data.

### Modified Capabilities

## Impact

New crate `crates/mcr-nbt`; depends on `mcr-buf`, `flate2`, `thiserror`; dev-dependency `criterion`.

Out of scope: text component modelling, codec/DataResult-style serialization, region file handling.
