# Tasks

## 1. Extractor

- [x] 1.1 Write `tools/extractor` and `run.sh`; running it produces blocks, items, entity types and fluids JSON in the version cache.

## 2. Generator

- [x] 2.1 Add `mcr-data-gen` reading reports and extracted JSON and writing the blob and Rust module; `cargo run -p mcr-data-gen` succeeds.
- [x] 2.2 Test generation determinism and full state coverage against blocks.json.

## 3. API

- [x] 3.1 Add `mcr-data` with `Version`, `v26_3()` and lookups; unit tests cover stone, air, glowstone, slab, login_finished.
- [x] 3.2 Run fmt, clippy and tests clean.
