# Design

## Context

Vanilla network NBT (since 1.20.2) is a type byte then the payload, no root name; type 0 means absent. Disk NBT has a root name. See proposal.md.

## Goals / Non-Goals

**Goals:** fast decode, bounded memory on hostile input, ergonomic getters.
**Non-Goals:** text components, codecs, region files.

## Decisions

- Compound is a `Vec<(String, Value)>` with linear lookup: compounds are small in practice and this keeps order and cache locality. Alternative (IndexMap) costs a dependency and hashing.
- Borrowed reader walks a `&[u8]` with skip and key lookup, comparing raw name bytes against the key's Modified UTF-8 form.
- Modified UTF-8 fast path: bytes without 0x00, 0xC0 or 0xED-surrogate leads are valid UTF-8 and are borrowed as-is; otherwise transcoded.
- Limits mirror `NbtAccounter` byte costs; preallocation is capped by remaining input length.
- SNBT follows the vanilla grammar subset: typed suffixes, `[B;]`, `[I;]`, `[L;]`, quoted and bare strings.

## Risks / Trade-offs

- Linear compound lookup is O(n) for large compounds; acceptable for NBT sizes seen, revisit if profiling says otherwise.
