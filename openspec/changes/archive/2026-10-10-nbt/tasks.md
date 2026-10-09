# Tasks

## 1. Crate and model

- [x] 1.1 Create `crates/mcr-nbt` with deps; `cargo check -p mcr-nbt` passes
- [x] 1.2 Implement `Value`, `Compound`, `List` with getters; unit tests pass

## 2. Codecs

- [x] 2.1 Modified UTF-8 encode/decode; edge-case tests pass
- [x] 2.2 Limited decoder/encoder, disk (with gzip/zlib) and network formats, `NetworkNbt` Encode/Decode; round-trip and hostile-input tests pass
- [x] 2.3 Borrowed reader with skip and key lookup; tests pass

## 3. SNBT

- [x] 3.1 SNBT formatter and parser; round-trip tests and ignored data-dir test pass

## 4. Bench

- [x] 4.1 Criterion bench decoding a large compound runs
