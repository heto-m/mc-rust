## 1. Stacks and patches

- [x] 1.1 `ItemStack`, `NonEmptyItemStack`, `UntrustedItemStack`, `ItemStackTemplate`, `DataComponentPatch`
- [x] 1.2 `HashedStack` with hashed patch map

## 2. Components

- [x] 2.1 `components!` table with all 122 types and registry agreement test
- [x] 2.2 Shared sub-types: `HolderSet`, enums, consume effects, mob effects
- [x] 2.3 Per-component structs, block predicates and component matchers
- [x] 2.4 Decode depth guard

## 3. Tests

- [x] 3.1 Byte-exact fixtures (plain stack, named sword, untrusted, hashed)
- [x] 3.2 Round trips for many components, ignoring those needing text, sound or particle codecs
- [x] 3.3 Hostile input tests
