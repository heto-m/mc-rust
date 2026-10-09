## 1. Crate and containers

- [x] 1.1 Scaffold `mcr-world` with ids, `WorldConfig`, errors
- [x] 1.2 Packed bit storage and paletted container decode/encode/get/set/unpack
- [x] 1.3 Tests: all palette kinds, bits 1..=15, indexing order, hostile input

## 2. Columns and store

- [x] 2.1 `ChunkSection`, light arrays, `ChunkColumn`
- [x] 2.2 `ChunkData::decode` and `LightData::decode` with NBT skipper
- [x] 2.3 `ChunkStore` with block lookup, updates and dirty set

## 3. Verification

- [x] 3.1 Criterion benches for get/set/unpack and 24-section column decode
- [x] 3.2 fmt, clippy, tests, archive
