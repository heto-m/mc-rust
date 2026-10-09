## 1. Protocol types
- [x] 1.1 Add BitSet, FixedBitSet, LengthPrefixed, Identifier, BlockPos, Angle, IdOr with tests and byte fixtures

## 2. Derive
- [x] 2.1 Create mcr-buf-derive crate with struct and enum derives and attributes
- [x] 2.2 Re-export from mcr-buf behind the `derive` feature
- [x] 2.3 Round-trip, tag error and attribute tests

## 3. Verification
- [x] 3.1 Criterion bench for derived struct and BlockPos
- [x] 3.2 fmt, clippy, test pass
