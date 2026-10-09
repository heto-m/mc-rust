# Design

## Context

Vanilla block facts depend on running game code (shape functions). The extractor calls the game's public APIs at runtime from the user's own jars and emits JSON; nothing from Mojang is committed.

## Goals / Non-Goals

**Goals:**
- Dense struct-of-arrays tables indexed by state id, deduplicated shapes.
- No version-specific types in the public API.

**Non-Goals:**
- Dynamic shapes needing world context (extracted with the empty getter at origin, like vanilla's cache).
- Running the extractor in CI.

## Decisions

- Generated output is a compact binary blob (`include_bytes!`) plus a small Rust module with typed accessors; the JSON is about 20MB so Rust source tables would compile slowly. Alternative: static arrays in source, rejected for compile time.
- The blob is a sectioned little-endian layout parsed once into borrowed slices on first use. Names are kept in a string pool; name lookups use sorted arrays with binary search (no extra dependency).
- `Version` is a struct of borrowed tables; other versions are sibling generated modules constructing the same struct.
- Shapes are f32 boxes; identical shape lists share an index.
- The generator takes explicit input and output paths so it can be tested on the real cache when present.

## Risks / Trade-offs

- Real data is gitignored, so generator tests needing the reports skip when absent; the committed blob is tested directly.
- Binary blob diffs are opaque; a determinism test guards regeneration.
